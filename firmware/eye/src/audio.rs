use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Bank, DmaReg, Udma, Uart, UartReg};
use bao1x_hal_service::UdmaGlobal;

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::{level_from_wire, norm_from_wire, EAR_UART_BAUD, FRAME_LEN, LEVEL_DB_FLOOR, MelFrame, SYNC_BYTE};
use triangel_shared::tuning::{beat::*, drop_detect::*, level::*, onset::*};

use crate::pins;

// --- UART init status - written during init, read by AudioFill for debug display ---
pub const STATUS_PENDING:    u8 = 0;
pub const STATUS_CSR_FAIL:   u8 = 1;
pub const STATUS_IFRAM_FAIL: u8 = 2;
pub const STATUS_INIT_OK:    u8 = 3;
pub const STATUS_DMA_DONE:   u8 = 4;
pub const STATUS_RECEIVING:  u8 = 5;
pub static UART_STATUS:        AtomicU8  = AtomicU8::new(STATUS_PENDING);
pub static UART_FIRST_BYTE:    AtomicU8  = AtomicU8::new(0);
pub static UART_LAST_FRAME_MS: AtomicU32 = AtomicU32::new(0);
/// Frames that decoded, and frames dropped on a bad sync or checksum. The pair tells
/// a wrong-data link from a dead one.
pub static UART_FRAMES_OK:     AtomicU32 = AtomicU32::new(0);
pub static UART_FRAMES_BAD:    AtomicU32 = AtomicU32::new(0);
/// Latest decoded level, as f32 bits: absolute dBFS and the ear's normalized 0.0-1.0.
pub static UART_LAST_DBFS:     AtomicU32 = AtomicU32::new(0);
pub static UART_LAST_NORM:     AtomicU32 = AtomicU32::new(0);
/// Drop detector state for the heartbeat: in a breakdown, drops so far, and the raw
/// bass level against its reference, both in dB as f32 bits.
pub static DROP_BREAKDOWN:     AtomicBool = AtomicBool::new(false);
pub static DROP_COUNT:         AtomicU32  = AtomicU32::new(0);
pub static DROP_BASS_DB:       AtomicU32  = AtomicU32::new(0);
pub static DROP_REF_DB:        AtomicU32  = AtomicU32::new(0);

/// Drop detector state for the diagnostic heartbeat: in a breakdown, drops detected,
/// the bass level, and the reference it is judged against.
#[allow(dead_code)] // only the bringup build's heartbeat reads it
pub fn drop_stats() -> (bool, u32, f32, f32) {
    (
        DROP_BREAKDOWN.load(Ordering::Relaxed),
        DROP_COUNT.load(Ordering::Relaxed),
        f32::from_bits(DROP_BASS_DB.load(Ordering::Relaxed)),
        f32::from_bits(DROP_REF_DB.load(Ordering::Relaxed)),
    )
}

/// Link state for the diagnostic heartbeat: status, frames decoded, frames dropped,
/// the first byte ever seen, current dBFS, and current normalized level.
#[allow(dead_code)] // only the bringup build's heartbeat reads it
pub fn stats() -> (u8, u32, u32, u8, f32, f32) {
    (
        UART_STATUS.load(Ordering::Relaxed),
        UART_FRAMES_OK.load(Ordering::Relaxed),
        UART_FRAMES_BAD.load(Ordering::Relaxed),
        UART_FIRST_BYTE.load(Ordering::Relaxed),
        f32::from_bits(UART_LAST_DBFS.load(Ordering::Relaxed)),
        f32::from_bits(UART_LAST_NORM.load(Ordering::Relaxed)),
    )
}

// The UART's IFRAM block is split 2048 TX + 2048 RX by the HAL (UART_RX_BUF_START /
// UART_RX_BUF_SIZE in bao1x-hal's uart.rs - private there, so mirrored here). The RX
// half is our DMA ring: the UDMA engine writes incoming bytes into it continuously
// (CFG_CONT wraps forever) and update() chases its write pointer once per render
// frame. Reception is entirely hardware-side: a CPU-serviced byte interface cannot
// keep up with 53-byte bursts at 1 Mbaud (10 us/byte) under a multitasking OS.
const RX_DMA_BUF_START: usize = 2048;
const RX_DMA_BUF_LEN:   usize = 2048;

// --- Beat detection ---
// A struck drum moves the whole spectrum at once, so the trigger is the total rise
// across every band, gated on the low bands holding energy to tell a kick from a hat.
// The threshold is a multiple of the running average rather than a level: every band
// moves a little every frame, so flux has a busy floor a fixed threshold sits under.

// --- Drop detection ---
// A drop is the bass returning after a breakdown, so it is found by the absence before
// it rather than by how loud it is. It runs on the raw bass level alone, not on the beat
// detector, so it does not inherit that detector's misses.
//
// The bass follower rides the kick peaks and holds across the gaps between them, so a
// single bar's rest does not read as the bass leaving.
const BASS_ATTACK: f32 = 0.5;
const BASS_DECAY: f32 = 0.05;
/// How fast the reference - the normal level of the bass - follows while music plays.
const BASS_REF_RATE: f32 = 0.008;

// Fall per quiet tick once the ear stops sending; drains the window in ~4 s.
const QUIET_DECAY_DB: f32 = 2.5;

/// Custom cache-flush instruction (from the baochip dma_basic2 test) so the CPU
/// re-reads the DMA engine's writes instead of a stale cached copy.
#[inline(always)]
fn cache_flush() {
    // Safety: a hint instruction with no memory operands of its own.
    unsafe {
        core::arch::asm!(".word 0x500F", "nop", "nop", "nop", "nop", "nop");
    }
}

/// One frame of audio as patterns see it.
#[derive(Clone, Copy)]
pub struct Audio {
    /// Loudness relative to recent music, 0.0-1.0.
    pub level_norm: f32,
    /// The 24 mel bands, 0.0-1.0, low frequency first. Keep their shape when quiet.
    pub bands:      [f32; MEL_BANDS],
    /// How far each band has just jumped above its own recent average, 0.0-1.0.
    pub rise:       [f32; MEL_BANDS],
    /// True on the one frame a beat was detected, with how far past the threshold it
    /// reached in `beat_strength`.
    pub beat:          bool,
    pub beat_strength: f32,
    /// True on the one frame a drop lands: the bass returning after a breakdown.
    pub drop:       bool,
}

struct AudioState {
    mel:            [f32; MEL_BANDS],
    band_fast:      [f32; MEL_BANDS],
    band_slow:      [f32; MEL_BANDS],
    flux:           f32,
    flux_avg:       f32,
    beat_armed:     bool,
    beat_pending:   bool,
    beat_strength:  f32,
    last_beat_ms:   u32,
    bass_db:        f32,
    bass_env:       f32,
    bass_ref:       f32,
    low_since:      Option<u32>,
    in_breakdown:   bool,
    breakdown_since: u32,
    drop_pending:   bool,
    level_norm:     f32,
    smoothed_dbfs:  f32,
    activity:       bool,
    last_loud:      bool, // freshest byte's verdict; persists across byte-less frames
    loud_ms:        f32,  // leaky accumulator of net loud time, 0..=ACTIVITY_ARM_MS
    last_tick_ms:   u32,
    last_update_ms: u32,
    // Frame assembler (owned by the render loop): the partial frame and fill position.
    frame_buf:      [u8; FRAME_LEN],
    frame_pos:      usize,
    first_byte_seen: bool,
}

impl AudioState {
    fn new() -> Self {
        AudioState {
            mel:            [0.0; MEL_BANDS],
            band_fast:      [0.0; MEL_BANDS],
            band_slow:      [0.0; MEL_BANDS],
            flux:           0.0,
            flux_avg:       0.0,
            beat_armed:     true,
            beat_pending:   false,
            beat_strength:  0.0,
            last_beat_ms:   0,
            bass_db:        LEVEL_DB_FLOOR,
            bass_env:       LEVEL_DB_FLOOR,
            bass_ref:       LEVEL_DB_FLOOR,
            low_since:      None,
            in_breakdown:   false,
            breakdown_since: 0,
            drop_pending:   false,
            level_norm:     0.0,
            smoothed_dbfs:  LEVEL_DB_FLOOR,
            activity:       false,
            last_loud:      false,
            loud_ms:        0.0,
            last_tick_ms:   0,
            last_update_ms: 0,
            frame_buf:      [0u8; FRAME_LEN],
            frame_pos:      0,
            first_byte_seen: false,
        }
    }
}

/// The mapped UART2 CSR + IFRAM addresses and our read position in the RX DMA ring.
struct DmaRx {
    csr_virt:   usize,
    ifram_virt: usize,
    tail:       usize, // next unread index within the 2048-byte RX ring
}

pub struct AudioReceiver {
    // Owned and mutated only by the render thread; the DMA engine is the only other
    // writer, and it touches nothing but the IFRAM ring.
    state: AudioState,
    // None if the UART never came up (sound-reactive then stays disabled; the LED loop
    // is unaffected).
    dma: Option<DmaRx>,
}

impl Default for AudioReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioReceiver {
    pub fn new() -> Self {
        AudioReceiver {
            state: AudioState::new(),
            dma:   init_audio_uart(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.state.activity
    }

    /// Track the bass against its normal level: enter a breakdown when it stays well
    /// below for long enough, and fire a drop when it comes back. Called once per
    /// received frame.
    fn detect_drop(&mut self, now_ms: u32) {
        let st = &mut self.state;
        let rate = if st.bass_db > st.bass_env { BASS_ATTACK } else { BASS_DECAY };
        st.bass_env += (st.bass_db - st.bass_env) * rate;

        if st.in_breakdown {
            let elapsed = now_ms.wrapping_sub(st.breakdown_since);
            let back = st.bass_env >= st.bass_ref - DROP_DB;
            let expired = elapsed > BREAKDOWN_MAX_MS;
            if back || expired {
                st.in_breakdown = false;
                st.low_since = None;
                if back {
                    st.drop_pending = true;
                    DROP_COUNT.fetch_add(1, Ordering::Relaxed);
                } else {
                    // The music stopped. Let the quiet become the new normal rather than
                    // re-entering a breakdown against a level that has gone.
                    st.bass_ref = st.bass_env;
                }
            }
        } else {
            let low = st.bass_ref > BASS_MIN_DB && st.bass_env < st.bass_ref - BREAKDOWN_DB;
            // Only learn the normal level while the bass is present. Following it down
            // during the seconds it takes to confirm a breakdown closes the gap before the
            // confirmation can finish.
            if !low {
                st.bass_ref += (st.bass_env - st.bass_ref) * BASS_REF_RATE;
            }
            match (low, st.low_since) {
                (false, _) => st.low_since = None,
                (true, None) => st.low_since = Some(now_ms),
                (true, Some(since)) => {
                    if now_ms.wrapping_sub(since) >= BREAKDOWN_MS {
                        st.in_breakdown = true;
                        // The breakdown counts from when the bass first left, not from
                        // when it was confirmed.
                        st.breakdown_since = since;
                    }
                }
            }
        }

        DROP_BREAKDOWN.store(st.in_breakdown, Ordering::Relaxed);
        DROP_BASS_DB.store(st.bass_env.to_bits(), Ordering::Relaxed);
        DROP_REF_DB.store(st.bass_ref.to_bits(), Ordering::Relaxed);
    }

    /// Fire a beat when this frame's flux stands well above its recent average and the
    /// low bands hold energy. Called once per received frame, which is when flux moves.
    fn detect_beat(&mut self, now_ms: u32) {
        let st = &mut self.state;
        st.flux_avg += (st.flux - st.flux_avg) * FLUX_AVG_RATE;
        let low = st.mel[..KICK_BANDS].iter().sum::<f32>() / KICK_BANDS as f32;
        let ratio = if low >= LOW_MIN && st.flux_avg > 1e-4 { st.flux / st.flux_avg } else { 0.0 };

        if st.beat_armed
            && ratio >= FLUX_TRIGGER
            && now_ms.wrapping_sub(st.last_beat_ms) >= BEAT_REFRACTORY_MS
        {
            st.beat_armed = false;
            st.beat_strength = ((ratio - FLUX_TRIGGER) / FLUX_TRIGGER).clamp(0.0, 1.0);
            st.beat_pending = true;
            st.last_beat_ms = now_ms;
        } else if ratio < FLUX_RELEASE {
            st.beat_armed = true;
        }
    }

    /// The frame patterns render against.
    pub fn snapshot(&mut self) -> Audio {
        let mut rise = [0.0; MEL_BANDS];
        for (r, (fast, slow)) in rise
            .iter_mut()
            .zip(self.state.band_fast.iter().zip(self.state.band_slow.iter()))
        {
            *r = ((fast - slow) * RISE_GAIN).clamp(0.0, 1.0);
        }
        let beat = self.state.beat_pending;
        self.state.beat_pending = false;
        let drop = self.state.drop_pending;
        self.state.drop_pending = false;
        Audio {
            beat,
            drop,
            beat_strength: self.state.beat_strength,
            level_norm: self.state.level_norm,
            bands:      self.state.mel,
            rise,
        }
    }

    /// Called once per frame from the render loop. Chases the DMA engine's write pointer
    /// through the frame assembler, applies any complete frame, decays toward silence
    /// when the ear stops sending, and advances the slow arm/release accumulator.
    pub fn update(&mut self, now_ms: u32) {
        // Frame delta for the accumulator, capped so boot delay or a frame overrun
        // can't slam it forward in one step.
        let dt_ms = (now_ms.wrapping_sub(self.state.last_tick_ms) as f32).min(100.0);
        self.state.last_tick_ms = now_ms;

        // Drain every byte the DMA engine wrote since last frame through the state machine.
        let mut got_frame = false;
        let mut got_byte = false;
        if let Some(pos) = self.dma.as_ref().and_then(|d| d.write_pos()) {
            cache_flush();
            let mut tail = self.dma.as_ref().unwrap().tail;
            while tail != pos {
                let byte = self.dma.as_ref().unwrap().read_ring(tail);
                tail = (tail + 1) % RX_DMA_BUF_LEN;
                got_byte = true;
                if !self.state.first_byte_seen {
                    self.state.first_byte_seen = true;
                    UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
                }
                if self.feed_byte(byte, now_ms) {
                    got_frame = true;
                }
            }
            self.dma.as_mut().unwrap().tail = tail;
        }

        // Bytes arriving but nothing decoding is a different fault from silence, so
        // give it its own status rather than leaving it as "waiting for the first".
        if got_byte && UART_STATUS.load(Ordering::Relaxed) == STATUS_INIT_OK {
            UART_STATUS.store(STATUS_DMA_DONE, Ordering::Relaxed);
        }

        // No fresh frame for a while: the ear stopped sending - decay toward silence
        // and count the time as quiet.
        if !got_frame && now_ms.wrapping_sub(self.state.last_update_ms) >= 200 {
            self.state.smoothed_dbfs =
                (self.state.smoothed_dbfs - QUIET_DECAY_DB).max(LEVEL_DB_FLOOR);
            self.state.last_loud = false;
            self.state.last_update_ms = now_ms;
        }

        // Leaky accumulator: fill 1:1 while loud, drain at ARM/RELEASE while quiet.
        // Activity flips only at the rails, so borderline sound holds the current mode.
        if self.state.last_loud {
            self.state.loud_ms = (self.state.loud_ms + dt_ms).min(ACTIVITY_ARM_MS);
            if self.state.loud_ms >= ACTIVITY_ARM_MS {
                self.state.activity = true;
            }
        } else {
            let drain = dt_ms * (ACTIVITY_ARM_MS / ACTIVITY_RELEASE_MS);
            self.state.loud_ms = (self.state.loud_ms - drain).max(0.0);
            if self.state.loud_ms <= 0.0 {
                self.state.activity = false;
            }
        }
    }

    /// Feed one received byte into the frame assembler. Returns true when a complete,
    /// checksum-valid `MelFrame` was decoded and applied.
    fn feed_byte(&mut self, byte: u8, now_ms: u32) -> bool {
        if self.state.frame_pos == 0 {
            // Hunt for the sync byte; ignore anything else.
            if byte == SYNC_BYTE {
                self.state.frame_buf[0] = byte;
                self.state.frame_pos = 1;
            }
            return false;
        }
        self.state.frame_buf[self.state.frame_pos] = byte;
        self.state.frame_pos += 1;
        if self.state.frame_pos < FRAME_LEN {
            return false;
        }
        // Full frame collected. Reset for the next one, then validate.
        self.state.frame_pos = 0;
        // Copy out of the state buffer so the borrow ends before apply_frame's &mut self.
        let buf = self.state.frame_buf;
        if let Some(frame) = MelFrame::decode(&buf) {
            self.apply_frame(&frame, now_ms);
            true
        } else {
            // Bad checksum (we locked onto a 0xAA inside the data). Drop it; the
            // stream self-resyncs on the next real sync byte.
            UART_FRAMES_BAD.fetch_add(1, Ordering::Relaxed);
            false
        }
    }

    /// Apply a decoded frame: the 24 bands, the render level (light EMA), and the loud
    /// flag. The frame's own activity flag is available but not consumed yet - the
    /// eye's slow arm/release accumulator judges the level, unchanged from before.
    fn apply_frame(&mut self, frame: &MelFrame, now_ms: u32) {
        for (i, &b) in frame.bands.iter().enumerate() {
            let v = b as f32 / 65535.0;
            self.state.mel[i] = v;
            self.state.band_fast[i] += (v - self.state.band_fast[i]) * BAND_FAST;
            self.state.band_slow[i] += (v - self.state.band_slow[i]) * BAND_SLOW;
        }
        self.state.flux = norm_from_wire(frame.flux);
        self.state.bass_db = level_from_wire(frame.bass);
        self.detect_beat(now_ms);
        self.detect_drop(now_ms);
        self.state.level_norm = norm_from_wire(frame.level_norm);
        let dbfs = level_from_wire(frame.level);
        // Light EMA so one rogue frame can't spike the fill. In dB: even steps.
        self.state.smoothed_dbfs = self.state.smoothed_dbfs * 0.6 + dbfs * 0.4;
        self.state.last_loud = self.state.smoothed_dbfs > ACTIVITY_LOUD_DBFS;
        self.state.last_update_ms = now_ms;
        UART_STATUS.store(STATUS_RECEIVING, Ordering::Relaxed);
        UART_LAST_FRAME_MS.store(now_ms, Ordering::Relaxed);
        UART_FRAMES_OK.fetch_add(1, Ordering::Relaxed);
        UART_LAST_DBFS.store(self.state.smoothed_dbfs.to_bits(), Ordering::Relaxed);
        UART_LAST_NORM.store(self.state.level_norm.to_bits(), Ordering::Relaxed);
    }
}

impl DmaRx {
    /// The DMA engine's live write position within the RX ring, derived from the RX
    /// channel's SIZE register - the countdown of bytes remaining in the current pass,
    /// which decrements as bytes land (and reloads to the full size on each CONT wrap).
    /// SADDR does not read back as a live pointer on this chip, so SIZE is the source.
    /// None if the readback is out of range (transfer idle or mid-reload edge).
    fn write_pos(&self) -> Option<usize> {
        // Safety: Bank::Rx + DmaReg::Size of the mapped UART CSR page.
        let remaining = unsafe {
            (self.csr_virt as *const u32).add(Bank::Rx as usize + DmaReg::Size as usize).read_volatile()
        } as usize;
        if remaining == 0 || remaining > RX_DMA_BUF_LEN {
            return None;
        }
        Some((RX_DMA_BUF_LEN - remaining) % RX_DMA_BUF_LEN)
    }

    /// Read ring byte `idx` through the virtual IFRAM mapping.
    fn read_ring(&self, idx: usize) -> u8 {
        // Safety: idx is bounded by RX_DMA_BUF_LEN within the mapped 4K IFRAM page.
        unsafe { ((self.ifram_virt + RX_DMA_BUF_START) as *const u8).add(idx).read_volatile() }
    }
}

/// Maps UART2, switches its RX path to continuous DMA into the IFRAM ring, and returns
/// the mapped addresses. None if the UART never came up.
fn init_audio_uart() -> Option<DmaRx> {
    let tt = ticktimer::Ticktimer::new().unwrap();
    let iox = IoxHal::new();
    pins::setup_input_pin(&iox, pins::AUDIO_UART_RX_PORT, pins::AUDIO_UART_RX_PIN, IoxFunction::AF1, IoxEnable::Enable);
    UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

    // UART2 may be transiently owned by another process at boot; retry with backoff for a
    // few seconds rather than dying on the first failure. If it never comes up, run with
    // sound-reactive disabled - a boot-time conflict won't resolve later anyway.
    const MAX_INIT_ATTEMPTS: u32 = 50; // ~5s at 100ms backoff
    let mut attempt = 0u32;
    loop {
        if let Some(dma) = init_uart() {
            if attempt > 0 {
                log::info!("audio UART init recovered after {} retries", attempt);
            }
            return Some(dma);
        }
        if attempt == 0 {
            log::warn!("audio UART init failed (status {}); retrying", UART_STATUS.load(Ordering::Relaxed));
        }
        attempt += 1;
        if attempt >= MAX_INIT_ATTEMPTS {
            log::error!("audio UART init failed after {} attempts; sound-reactive disabled", MAX_INIT_ATTEMPTS);
            return None;
        }
        tt.sleep_ms(100).ok();
    }
}

/// Maps UART2's CSR + IFRAM, sets the baud, switches RX to streaming (DMA) mode, and
/// starts the continuous ring transfer. Returns None (with UART_STATUS set) on failure.
fn init_uart() -> Option<DmaRx> {
    let csr_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(utralib::utra::udma_uart_2::HW_UDMA_UART_2_BASE),
        None, 4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_CSR_FAIL, Ordering::Relaxed); return None; }
    };

    let ifram_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(bao1x_hal::board::APP_UART_IFRAM_ADDR),
        None, 4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_IFRAM_FAIL, Ordering::Relaxed); return None; }
    };

    let csr_virt   = csr_mem.as_ptr() as usize;
    let ifram_virt = ifram_mem.as_ptr() as usize;
    let _ = (csr_mem, ifram_mem);

    let uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, ifram_virt)
    };
    uart.set_baud(EAR_UART_BAUD, PERCLK_HZ);

    // set_baud leaves the UART in poll mode (Setup bit 0x10: bytes go to the 1-deep
    // Valid/Data command interface). Rewrite Setup without that bit so RX streams into
    // the UDMA engine instead. Same disable-then-configure sequence set_baud uses.
    let clk_counter: u32 = PERCLK_HZ / EAR_UART_BAUD;
    // Safety: Bank::Custom + UartReg::Setup is the Setup register of the mapped UART CSR.
    unsafe {
        let setup = (csr_virt as *mut u32).add(Bank::Custom as usize + UartReg::Setup as usize);
        setup.write_volatile(0);
        setup.write_volatile(0x0306 | (clk_counter << 16));
    }

    // Start the continuous RX transfer over the whole 2048-byte RX half of the IFRAM
    // block: the engine wraps forever (0b1 = the HAL's CFG_CONT; udma_enqueue ORs in
    // its own enable bit) and we chase its write pointer from update().
    // Safety: the slice describes the physical RX region; only its address/len are used.
    unsafe {
        let rx_phys = core::slice::from_raw_parts(
            (bao1x_hal::board::APP_UART_IFRAM_ADDR + RX_DMA_BUF_START) as *const u8,
            RX_DMA_BUF_LEN,
        );
        uart.udma_enqueue(Bank::Rx, rx_phys, 0b1);
    }

    UART_STATUS.store(STATUS_INIT_OK, Ordering::Relaxed);
    let dma = DmaRx { csr_virt, ifram_virt, tail: 0 };
    // Start reading from wherever the engine is now, not from index 0.
    let tail = dma.write_pos().unwrap_or(0);
    Some(DmaRx { tail, ..dma })
}
