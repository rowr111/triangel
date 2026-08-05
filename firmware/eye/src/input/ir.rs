use std::sync::atomic::{AtomicU32, Ordering};

use super::{EventQueue, InputEvent};
use crate::diag::Diag;
use crate::input::nec_capture::{NecCapture, REPEAT_FRAME};
use crate::pins;

// Extended NEC: the address bytes are a fixed ID, so only the command pair
// is checksummed.
const NEC_ADDR_LO: u8 = 0x85;
const NEC_ADDR_HI: u8 = 0xFE;

// Command bytes from the 7-button remote.
const IR_CMD_BRIGHTNESS_UP:   u8 = 0x43; // Up button
const IR_CMD_BRIGHTNESS_DOWN: u8 = 0x44; // Down button
const IR_CMD_PATTERN_NEXT:    u8 = 0x41; // Right button
const IR_CMD_PATTERN_PREV:    u8 = 0x42; // Left button
const IR_CMD_HOLD:            u8 = 0x40; // Center button
const IR_CMD_GEAR:            u8 = 0x46; // Gear button -> cycle sound mode
#[allow(dead_code)]
const IR_CMD_TV:              u8 = 0x45; // TV button - spare (use TBD)

// Diagnostic state reported by the heartbeat line.
static CLOCK_HZ:        AtomicU32 = AtomicU32::new(0);
static DECODED_FRAMES:  AtomicU32 = AtomicU32::new(0);
static REJECTED_FRAMES: AtomicU32 = AtomicU32::new(0);
static LAST_FRAME:      AtomicU32 = AtomicU32::new(0);

/// (clock_hz, decoded, rejected, last_frame) for the heartbeat line.
#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub fn stats() -> (u32, u32, u32, u32) {
    (
        CLOCK_HZ.load(Ordering::Relaxed),
        DECODED_FRAMES.load(Ordering::Relaxed),
        REJECTED_FRAMES.load(Ordering::Relaxed),
        LAST_FRAME.load(Ordering::Relaxed),
    )
}

/// Spawn the IR receiver thread; init progress prints to the USB serial monitor.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || {
        let diag = Diag::new();
        diag.line("IR: thread start, initializing BIO capture");
        let pin = arbitrary_int::u5::new(pins::IR_BIO_PIN);
        match NecCapture::new(pin) {
            Ok(capture) => {
                CLOCK_HZ.store(capture.clock_hz(), Ordering::Relaxed);
                log::info!("IR capture on BIO pin {} at {} Hz", pins::IR_BIO_PIN, capture.clock_hz());
                diag.line(&format!(
                    "IR capture on BIO pin {} at {} Hz",
                    pins::IR_BIO_PIN,
                    capture.clock_hz()
                ));
                receive_loop(capture, queue, diag);
            }
            // Give up rather than panic: the d-pad still works without IR.
            Err(e) => {
                diag.line(&format!("IR NecCapture init FAILED: {:?}", e));
                log::error!("IR NecCapture init failed: {:?}", e);
            }
        }
    });
}

fn receive_loop(capture: NecCapture, queue: EventQueue, diag: Diag) -> ! {
    let tt = ticktimer::Ticktimer::new().unwrap();
    // Last valid command; repeat frames re-trigger it (brightness only).
    let mut last_cmd: Option<u8> = None;
    loop {
        while let Some(frame) = capture.try_read_frame() {
            if frame == REPEAT_FRAME {
                // Only brightness auto-repeats; other commands act once per press.
                if let Some(cmd @ (IR_CMD_BRIGHTNESS_UP | IR_CMD_BRIGHTNESS_DOWN)) = last_cmd {
                    map_ir_cmd(cmd, &queue);
                }
                continue;
            }
            LAST_FRAME.store(frame, Ordering::Relaxed);
            let [addr_lo, addr_hi, cmd, cmd_inv] = frame.to_le_bytes();
            if addr_lo != NEC_ADDR_LO || addr_hi != NEC_ADDR_HI || (cmd ^ cmd_inv) != 0xFF {
                REJECTED_FRAMES.fetch_add(1, Ordering::Relaxed);
                diag.line(&format!("IR frame rejected: {:08x}", frame));
                log::debug!("IR frame rejected: {:08x}", frame);
                continue;
            }
            DECODED_FRAMES.fetch_add(1, Ordering::Relaxed);
            diag.line(&format!("IR frame ok: cmd {:02x} (frame {:08x})", cmd, frame));
            last_cmd = Some(cmd);
            map_ir_cmd(cmd, &queue);
        }
        // The FIFO holds eight frames, so this only bounds input latency.
        tt.sleep_ms(10).ok();
    }
}

/// Map a command byte to an InputEvent and queue it.
fn map_ir_cmd(cmd: u8, queue: &EventQueue) {
    let event = match cmd {
        IR_CMD_BRIGHTNESS_UP   => Some(InputEvent::BrightnessUp),
        IR_CMD_BRIGHTNESS_DOWN => Some(InputEvent::BrightnessDown),
        IR_CMD_PATTERN_NEXT    => Some(InputEvent::PatternNext),
        IR_CMD_PATTERN_PREV    => Some(InputEvent::PatternPrev),
        IR_CMD_HOLD            => Some(InputEvent::ToggleHold),
        IR_CMD_GEAR            => Some(InputEvent::CycleSoundMode),
        // TV button spare
        _                      => None,
    };
    if let Some(ev) = event {
        super::lock_queue(queue).push_back(ev);
    }
}
