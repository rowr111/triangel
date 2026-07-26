use std::sync::atomic::{AtomicU32, Ordering};

use super::{EventQueue, InputEvent};
use crate::diag::Diag;
use crate::input::pulse_capture::PulseCapture;
use crate::pins;

// IR receiver module data output: idle = HIGH, burst = LOW
// (module inverts and demodulates the 38kHz carrier).

// NEC protocol timing in microseconds.
const NEC_LEADER_SPACE_US: u32 = 4_500;
const NEC_BIT_PULSE_US:    u32 =   560;
const NEC_BIT_0_SPACE_US:  u32 =   560;
const NEC_BIT_1_SPACE_US:  u32 = 1_690;
const NEC_REPEAT_SPACE_US: u32 = 2_250;
const NEC_TIMING_MARGIN:   u32 =   200; // +/-us tolerance

// The capture reports the interval between consecutive RISING edges (the end
// of each LOW burst): one space plus the 560us burst that ends it. The four
// windows below stay disjoint at +/- NEC_TIMING_MARGIN.
const PERIOD_BIT0_US:   u32 = NEC_BIT_0_SPACE_US + NEC_BIT_PULSE_US;  // 1120
const PERIOD_BIT1_US:   u32 = NEC_BIT_1_SPACE_US + NEC_BIT_PULSE_US;  // 2250
const PERIOD_REPEAT_US: u32 = NEC_REPEAT_SPACE_US + NEC_BIT_PULSE_US; // 2810
const PERIOD_LEADER_US: u32 = NEC_LEADER_SPACE_US + NEC_BIT_PULSE_US; // 5060

// The remote is extended NEC: its two address bytes are a fixed ID (not a
// byte and its inverse), so only the command byte pair is checksummed.
const NEC_ADDR_LO: u8 = 0x85;
const NEC_ADDR_HI: u8 = 0xFE;

// Command bytes as sent by the 7-button remote. Sound mode is normally the
// physical switch's job; the gear button also cycles it.
const IR_CMD_BRIGHTNESS_UP:   u8 = 0x43; // Up button
const IR_CMD_BRIGHTNESS_DOWN: u8 = 0x44; // Down button
const IR_CMD_PATTERN_NEXT:    u8 = 0x41; // Right button
const IR_CMD_PATTERN_PREV:    u8 = 0x42; // Left button
const IR_CMD_HOLD:            u8 = 0x40; // Center button
const IR_CMD_GEAR:            u8 = 0x46; // Gear button -> cycle sound mode
#[allow(dead_code)]
const IR_CMD_TV:              u8 = 0x45; // TV button - spare (use TBD)

// Diagnostic state reported by the heartbeat line.
static CLOCK_HZ:        AtomicU32  = AtomicU32::new(0);
static DECODED_FRAMES:  AtomicU32  = AtomicU32::new(0);
static REJECTED_FRAMES: AtomicU32  = AtomicU32::new(0);
static LAST_FRAME:      AtomicU32  = AtomicU32::new(0);
static EDGE_COUNT:      AtomicU32  = AtomicU32::new(0);
// Ring snapshot published by the drain loop each pass (see PulseCapture::debug_state).
static RING_HEAD:       AtomicU32  = AtomicU32::new(0);
static RING_TAIL:       AtomicU32  = AtomicU32::new(0);
static RING_MAGIC:      AtomicU32  = AtomicU32::new(0);

/// (head, tail, magic) as of the drain loop's last pass, for the heartbeat.
#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub fn ring_stats() -> (u32, u32, u32) {
    (
        RING_HEAD.load(Ordering::Relaxed),
        RING_TAIL.load(Ordering::Relaxed),
        RING_MAGIC.load(Ordering::Relaxed),
    )
}

/// (clock_hz, decoded, rejected, last_frame, edges) for the heartbeat line.
#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub fn stats() -> (u32, u32, u32, u32, u32) {
    (
        CLOCK_HZ.load(Ordering::Relaxed),
        DECODED_FRAMES.load(Ordering::Relaxed),
        REJECTED_FRAMES.load(Ordering::Relaxed),
        LAST_FRAME.load(Ordering::Relaxed),
        EDGE_COUNT.load(Ordering::Relaxed),
    )
}

/// Spawn the IR receiver thread. Init progress prints to the USB serial
/// monitor so a stall or failure is visible.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || {
        let diag = Diag::new();
        diag.line("IR: thread start, initializing BIO capture");
        let pin = arbitrary_int::u5::new(pins::IR_BIO_PIN);
        match PulseCapture::new(pin, &|s| diag.line(s)) {
            Ok(capture) => {
                CLOCK_HZ.store(capture.clock_hz(), Ordering::Relaxed);
                log::info!("IR capture on BIO pin {} at {} Hz", pins::IR_BIO_PIN, capture.clock_hz());
                diag.line(&format!(
                    "IR capture on BIO pin {} at {} Hz",
                    pins::IR_BIO_PIN,
                    capture.clock_hz()
                ));
                decode_loop(capture, queue, diag);
            }
            // Report and give up rather than panic: the d-pad still works without IR.
            Err(e) => {
                diag.line(&format!("IR PulseCapture init FAILED: {:?}", e));
                log::error!("IR PulseCapture init failed: {:?}", e);
            }
        }
    });
}

fn decode_loop(mut capture: PulseCapture, queue: EventQueue, diag: Diag) -> ! {
    let tt = ticktimer::Ticktimer::new().unwrap();
    let mut decoder = NecDecoder::new();
    // The decoder prints only at frame boundaries: one line per completed,
    // rejected, or aborted frame, sent while the IR line is idle.
    loop {
        while let Some(period_us) = capture.try_read_us() {
            EDGE_COUNT.fetch_add(1, Ordering::Relaxed);
            decoder.feed(period_us, &queue, &diag);
        }
        let (head, tail, magic) = capture.debug_state();
        RING_HEAD.store(head, Ordering::Relaxed);
        RING_TAIL.store(tail, Ordering::Relaxed);
        RING_MAGIC.store(magic, Ordering::Relaxed);
        // Drain interval: the ring buffers seconds of edges, so this cadence
        // only bounds input latency, not capture correctness.
        tt.sleep_ms(10).ok();
    }
}

enum IrState {
    Idle,
    Data { bits: u32, count: u8 },
}

// Leader + 32 bit intervals, with headroom.
const HISTORY_LEN: usize = 40;

struct NecDecoder {
    state: IrState,
    /// Last valid command byte; repeat frames re-trigger it (brightness only).
    last_cmd: Option<u8>,
    /// Intervals of the frame in progress, reported when the frame ends.
    history: [u32; HISTORY_LEN],
    hist_len: usize,
}

impl NecDecoder {
    fn new() -> Self {
        Self { state: IrState::Idle, last_cmd: None, history: [0; HISTORY_LEN], hist_len: 0 }
    }

    fn push_history(&mut self, period_us: u32) {
        if self.hist_len < HISTORY_LEN {
            self.history[self.hist_len] = period_us;
            self.hist_len += 1;
        }
    }

    fn history_line(&self) -> String {
        let mut line = String::from("intervals us:");
        for &us in &self.history[..self.hist_len] {
            line.push_str(&format!(" {}", us));
        }
        line
    }

    fn feed(&mut self, period_us: u32, queue: &EventQueue, diag: &Diag) {
        let hit = |center: u32| {
            period_us >= center - NEC_TIMING_MARGIN && period_us <= center + NEC_TIMING_MARGIN
        };
        // A leader always starts a fresh frame, whatever state we were in.
        if hit(PERIOD_LEADER_US) {
            self.state = IrState::Data { bits: 0, count: 0 };
            self.hist_len = 0;
            self.push_history(period_us);
            return;
        }
        match self.state {
            IrState::Data { bits, count } => {
                self.push_history(period_us);
                let bit = if hit(PERIOD_BIT0_US) {
                    0u32
                } else if hit(PERIOD_BIT1_US) {
                    1u32
                } else {
                    // Stray interval (missed edge, noise): drop the partial
                    // frame and resync on the next leader.
                    self.state = IrState::Idle;
                    diag.line(&format!(
                        "IR frame aborted after {} bits, {}",
                        count,
                        self.history_line()
                    ));
                    return;
                };
                // Bits arrive LSB first; shifting each into the top bit makes
                // the finished frame read [addr_lo, addr_hi, cmd, ~cmd] in
                // to_le_bytes() order.
                let bits = (bits >> 1) | (bit << 31);
                if count == 31 {
                    self.state = IrState::Idle;
                    self.finish_frame(bits, queue, diag);
                } else {
                    self.state = IrState::Data { bits, count: count + 1 };
                }
            }
            IrState::Idle => {
                // Repeat frames arrive every ~108ms while a button is held.
                // Only brightness auto-repeats; other commands act once per press.
                if hit(PERIOD_REPEAT_US) {
                    if let Some(cmd @ (IR_CMD_BRIGHTNESS_UP | IR_CMD_BRIGHTNESS_DOWN)) = self.last_cmd {
                        map_ir_cmd(cmd, queue);
                    }
                }
            }
        }
    }

    /// Validate a completed 32-bit frame and dispatch its command.
    fn finish_frame(&mut self, frame: u32, queue: &EventQueue, diag: &Diag) {
        LAST_FRAME.store(frame, Ordering::Relaxed);
        let [addr_lo, addr_hi, cmd, cmd_inv] = frame.to_le_bytes();
        if addr_lo != NEC_ADDR_LO || addr_hi != NEC_ADDR_HI || (cmd ^ cmd_inv) != 0xFF {
            REJECTED_FRAMES.fetch_add(1, Ordering::Relaxed);
            diag.line(&format!("IR frame rejected: {:08x}, {}", frame, self.history_line()));
            log::debug!("IR frame rejected: {:08x}", frame);
            return;
        }
        DECODED_FRAMES.fetch_add(1, Ordering::Relaxed);
        diag.line(&format!("IR frame ok: cmd {:02x} (frame {:08x})", cmd, frame));
        self.last_cmd = Some(cmd);
        map_ir_cmd(cmd, queue);
    }
}

/// Translate a decoded NEC command byte into an InputEvent and push it to the queue.
fn map_ir_cmd(cmd: u8, queue: &EventQueue) {
    let event = match cmd {
        IR_CMD_BRIGHTNESS_UP   => Some(InputEvent::BrightnessUp),
        IR_CMD_BRIGHTNESS_DOWN => Some(InputEvent::BrightnessDown),
        IR_CMD_PATTERN_NEXT    => Some(InputEvent::PatternNext),
        IR_CMD_PATTERN_PREV    => Some(InputEvent::PatternPrev),
        IR_CMD_HOLD            => Some(InputEvent::ToggleHold),
        IR_CMD_GEAR            => Some(InputEvent::CycleSoundMode),
        // TV button spare - add mapping once use is decided
        _                      => None,
    };
    if let Some(ev) = event {
        super::lock_queue(queue).push_back(ev);
    }
}
