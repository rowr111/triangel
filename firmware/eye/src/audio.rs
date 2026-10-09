use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU8, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Bank, DmaReg, Udma, Uart, UartReg};
use bao1x_hal_service::UdmaGlobal;

use triangel_shared::frame::{level_from_wire, norm_from_wire, BandFrame, BAND_COUNT, EAR_UART_BAUD, Fed, FrameAssembler, LEVEL_DB_FLOOR};
use triangel_shared::follow;
use triangel_shared::tuning::{beat::*, drop_detect::*, level::*, link::*, onset::*, BASS_BANDS};

use crate::pins;

/// UART link status. The heartbeat reports it as its number.
#[derive(Clone, Copy)]
enum LinkStatus {
    Pending   = 0,
    CsrFail   = 1,
    IframFail = 2,
    InitOk    = 3,
    DmaDone   = 4,
    Receiving = 5,
}

impl LinkStatus {
    fn set(self) {
        UART_STATUS.store(self as u8, Ordering::Relaxed);
    }
}

pub static UART_STATUS:        AtomicU8  = AtomicU8::new(LinkStatus::Pending as u8);
pub static UART_FIRST_BYTE:    AtomicU8  = AtomicU8::new(0);
/// Frames that decoded, and frames dropped on a bad sync or checksum.
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

/// Drop detector state for the heartbeat.
#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub struct DropStats {
    pub in_breakdown: bool,
    pub drops:        u32,
    pub bass_db:      f32,
    pub ref_db:       f32,
}

#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub fn drop_stats() -> DropStats {
    DropStats {
        in_breakdown: DROP_BREAKDOWN.load(Ordering::Relaxed),
        drops:        DROP_COUNT.load(Ordering::Relaxed),
        bass_db:      f32::from_bits(DROP_BASS_DB.load(Ordering::Relaxed)),
        ref_db:       f32::from_bits(DROP_REF_DB.load(Ordering::Relaxed)),
    }
}

/// Link state for the heartbeat.
#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub struct LinkStats {
    pub status:     u8,
    pub frames_ok:  u32,
    pub frames_bad: u32,
    /// The first byte ever received.
    pub first_byte: u8,
    pub dbfs:       f32,
    pub norm:       f32,
}

#[cfg(all(feature = "usb", not(feature = "previewer")))]
pub fn stats() -> LinkStats {
    LinkStats {
        status:     UART_STATUS.load(Ordering::Relaxed),
        frames_ok:  UART_FRAMES_OK.load(Ordering::Relaxed),
        frames_bad: UART_FRAMES_BAD.load(Ordering::Relaxed),
        first_byte: UART_FIRST_BYTE.load(Ordering::Relaxed),
        dbfs:       f32::from_bits(UART_LAST_DBFS.load(Ordering::Relaxed)),
        norm:       f32::from_bits(UART_LAST_NORM.load(Ordering::Relaxed)),
    }
}

// The HAL splits the UART's IFRAM block into 2048 TX and 2048 RX bytes; its constants are
// private, so they are mirrored here. The RX half is the DMA ring the UDMA engine fills,
// because reading a byte at a time on the CPU cannot keep up with 1 Mbaud bursts.
const RX_DMA_BUF_START: usize = 2048;
const RX_DMA_BUF_LEN:   usize = 2048;

// The bass follower rises with the kicks and holds across the gaps between them, so a
// single bar's rest does not read as the bass leaving.
const BASS_ATTACK: f32 = 0.5;
const BASS_DECAY: f32 = 0.05;
/// How fast the reference, the normal level of the bass, follows while music plays.
const BASS_REF_RATE: f32 = 0.008;

// dB the loudness falls each STOPPED_AFTER_MS while the ear is not sending.
const QUIET_DECAY_DB: f32 = 2.5;

/// Custom cache-flush instruction, so the CPU re-reads what the DMA engine wrote.
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
    /// The 24 bands, 0.0-1.0, low frequency first. Keep their shape when quiet.
    pub bands:      [f32; BAND_COUNT],
    /// How far each band has just jumped above its own recent average, 0.0-1.0.
    pub rise:       [f32; BAND_COUNT],
    /// True on the one frame a beat was detected, with how far past the threshold it
    /// reached in `beat_strength`.
    pub beat:          bool,
    pub beat_strength: f32,
    /// True on the one frame a drop lands: the bass returning after a breakdown.
    pub drop:       bool,
}

/// The bands as last received, with each band's fast and slow averages.
struct Onsets {
    bands:     [f32; BAND_COUNT],
    band_fast: [f32; BAND_COUNT],
    band_slow: [f32; BAND_COUNT],
}

impl Onsets {
    fn new() -> Self {
        Onsets {
            bands:     [0.0; BAND_COUNT],
            band_fast: [0.0; BAND_COUNT],
            band_slow: [0.0; BAND_COUNT],
        }
    }

    /// Take a frame's bands.
    fn apply(&mut self, wire: &[u16; BAND_COUNT]) {
        for (i, &b) in wire.iter().enumerate() {
            let v = norm_from_wire(b);
            self.bands[i] = v;
            self.band_fast[i] += (v - self.band_fast[i]) * BAND_FAST;
            self.band_slow[i] += (v - self.band_slow[i]) * BAND_SLOW;
        }
    }

    /// Mean of the bass bands.
    fn bass(&self) -> f32 {
        self.bands[..BASS_BANDS].iter().sum::<f32>() / BASS_BANDS as f32
    }

    /// How far each band has just jumped above its slow average, 0.0-1.0.
    fn rise(&self) -> [f32; BAND_COUNT] {
        let mut rise = [0.0; BAND_COUNT];
        for (r, (fast, slow)) in rise
            .iter_mut()
            .zip(self.band_fast.iter().zip(self.band_slow.iter()))
        {
            *r = ((fast - slow) * RISE_GAIN).clamp(0.0, 1.0);
        }
        rise
    }

    /// Scale the bands toward silence, keeping the fraction `keep`.
    fn fade(&mut self, keep: f32) {
        for ((band, fast), slow) in self.bands.iter_mut().zip(&mut self.band_fast).zip(&self.band_slow) {
            *band *= keep;
            // Toward the slow baseline, which is kept, so `rise` falls to zero now and
            // does not jump when frames return.
            *fast = slow + (*fast - slow) * keep;
        }
    }
}

/// Fires a beat when a frame's flux stands well above its recent average and the bass
/// bands hold energy.
struct BeatDetector {
    flux_avg: f32,
    armed:    bool,
    pending:  bool,
    strength: f32,
    last_ms:  u32,
}

impl BeatDetector {
    fn new() -> Self {
        BeatDetector { flux_avg: 0.0, armed: true, pending: false, strength: 0.0, last_ms: 0 }
    }

    /// Called once per received frame, which is when flux moves. `low` is the bass level.
    fn update(&mut self, flux: f32, low: f32, now_ms: u32) {
        self.flux_avg += (flux - self.flux_avg) * FLUX_AVG_RATE;
        let ratio = if low >= LOW_MIN && self.flux_avg > 1e-4 { flux / self.flux_avg } else { 0.0 };

        if self.armed
            && ratio >= FLUX_TRIGGER
            && now_ms.wrapping_sub(self.last_ms) >= BEAT_REFRACTORY_MS
        {
            self.armed = false;
            self.strength = ((ratio - FLUX_TRIGGER) / FLUX_TRIGGER).clamp(0.0, 1.0);
            self.pending = true;
            self.last_ms = now_ms;
        } else if ratio < FLUX_RELEASE {
            self.armed = true;
        }
    }
}

/// Tracks the bass against its normal level: enters a breakdown when it stays well below
/// for long enough, and fires a drop when it comes back.
struct DropDetector {
    bass_env:        f32,
    bass_ref:        f32,
    low_since:       Option<u32>,
    in_breakdown:    bool,
    breakdown_since: u32,
    pending:         bool,
}

impl DropDetector {
    fn new() -> Self {
        DropDetector {
            bass_env:        LEVEL_DB_FLOOR,
            bass_ref:        LEVEL_DB_FLOOR,
            low_since:       None,
            in_breakdown:    false,
            breakdown_since: 0,
            pending:         false,
        }
    }

    /// Called once per received frame with the frame's bass level in dB.
    fn update(&mut self, bass_db: f32, now_ms: u32) {
        self.bass_env = follow(self.bass_env, bass_db, BASS_ATTACK, BASS_DECAY);

        if self.in_breakdown {
            let elapsed = now_ms.wrapping_sub(self.breakdown_since);
            let back = self.bass_env >= self.bass_ref - DROP_DB;
            let expired = elapsed > BREAKDOWN_MAX_MS;
            if back || expired {
                self.in_breakdown = false;
                self.low_since = None;
                if back {
                    self.pending = true;
                    DROP_COUNT.fetch_add(1, Ordering::Relaxed);
                } else {
                    // The music stopped. Let the quiet become the new normal rather than
                    // re-entering a breakdown against a level that has gone.
                    self.bass_ref = self.bass_env;
                }
            }
        } else {
            let low = self.bass_ref > BASS_MIN_DB && self.bass_env < self.bass_ref - BREAKDOWN_DB;
            // Learn the normal level only while the bass is present. Following it down would
            // close the gap before a breakdown is confirmed.
            if !low {
                self.bass_ref += (self.bass_env - self.bass_ref) * BASS_REF_RATE;
            }
            match (low, self.low_since) {
                (false, _) => self.low_since = None,
                (true, None) => self.low_since = Some(now_ms),
                (true, Some(since)) => {
                    if now_ms.wrapping_sub(since) >= BREAKDOWN_MS {
                        self.in_breakdown = true;
                        // The breakdown counts from when the bass first left, not from
                        // when it was confirmed.
                        self.breakdown_since = since;
                    }
                }
            }
        }

        DROP_BREAKDOWN.store(self.in_breakdown, Ordering::Relaxed);
        DROP_BASS_DB.store(self.bass_env.to_bits(), Ordering::Relaxed);
        DROP_REF_DB.store(self.bass_ref.to_bits(), Ordering::Relaxed);
    }
}

/// Decides when Auto mode shows the sound patterns, from how long it has been loud.
struct ActivityGate {
    dbfs:      f32,  // as the ear sent it; the ear does the smoothing
    last_loud: bool, // the latest frame's verdict; kept until the next frame or quiet tick
    loud_ms:   f32,  // net loud time, 0..=ACTIVITY_ARM_MS
    active:    bool,
}

impl ActivityGate {
    fn new() -> Self {
        ActivityGate { dbfs: LEVEL_DB_FLOOR, last_loud: false, loud_ms: 0.0, active: false }
    }

    /// Take a frame's loudness.
    fn frame(&mut self, dbfs: f32) {
        self.dbfs = dbfs;
        self.last_loud = dbfs > ACTIVITY_LOUD_DBFS;
    }

    /// The ear has sent nothing for another STOPPED_AFTER_MS.
    fn quiet(&mut self) {
        self.dbfs = (self.dbfs - QUIET_DECAY_DB).max(LEVEL_DB_FLOOR);
        self.last_loud = false;
    }

    /// Advance by one render frame. Fills at 1:1 while loud and drains at ARM/RELEASE while
    /// quiet. `active` changes only when full or empty, so borderline sound holds the mode.
    fn tick(&mut self, dt_ms: f32) {
        if self.last_loud {
            self.loud_ms = (self.loud_ms + dt_ms).min(ACTIVITY_ARM_MS);
            if self.loud_ms >= ACTIVITY_ARM_MS {
                self.active = true;
            }
        } else {
            let drain = dt_ms * (ACTIVITY_ARM_MS / ACTIVITY_RELEASE_MS);
            self.loud_ms = (self.loud_ms - drain).max(0.0);
            if self.loud_ms <= 0.0 {
                self.active = false;
            }
        }
    }
}

/// The mapped UART2 CSR + IFRAM addresses and our read position in the RX DMA ring.
struct DmaRx {
    csr_virt:   usize,
    ifram_virt: usize,
    tail:       usize, // next unread index within the 2048-byte RX ring
}

// Owned and mutated only by the render thread; the DMA engine is the only other writer,
// and it touches nothing but the IFRAM ring.
pub struct AudioReceiver {
    onsets:          Onsets,
    beats:           BeatDetector,
    drops:           DropDetector,
    gate:            ActivityGate,
    level_norm:      f32,
    last_tick_ms:    u32,
    last_update_ms:  u32,
    ear_stopped:     bool, // no frame for STOPPED_AFTER_MS; cleared by the next one
    assembler:       FrameAssembler,
    first_byte_seen: bool,
    // None if the UART never came up; sound-reactive then stays off.
    dma:             Option<DmaRx>,
}

impl AudioReceiver {
    pub fn new() -> Self {
        AudioReceiver {
            onsets:          Onsets::new(),
            beats:           BeatDetector::new(),
            drops:           DropDetector::new(),
            gate:            ActivityGate::new(),
            level_norm:      0.0,
            last_tick_ms:    0,
            last_update_ms:  0,
            ear_stopped:     false,
            assembler:       FrameAssembler::default(),
            first_byte_seen: false,
            dma:             init_audio_uart(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.gate.active
    }

    /// The frame patterns render against.
    pub fn snapshot(&mut self) -> Audio {
        Audio {
            beat:          std::mem::take(&mut self.beats.pending),
            drop:          std::mem::take(&mut self.drops.pending),
            beat_strength: self.beats.strength,
            level_norm:    self.level_norm,
            bands:         self.onsets.bands,
            rise:          self.onsets.rise(),
        }
    }

    /// Called once per render frame. Applies the frames that have arrived, fades toward
    /// silence when the ear stops sending, and advances the Auto-mode timer.
    pub fn update(&mut self, now_ms: u32) {
        // Time since the last call, capped so a long gap cannot jump the timers ahead.
        let dt_ms = (now_ms.wrapping_sub(self.last_tick_ms) as f32).min(100.0);
        self.last_tick_ms = now_ms;

        // Feed every byte the DMA engine wrote since the last call to the assembler.
        let mut got_frame = false;
        let mut got_byte = false;
        // `dma` is taken out for the loop, because feed_byte needs all of `self`.
        if let Some(mut dma) = self.dma.take() {
            if let Some(pos) = dma.write_pos() {
                cache_flush();
                while dma.tail != pos {
                    let byte = dma.read_ring(dma.tail);
                    dma.tail = (dma.tail + 1) % RX_DMA_BUF_LEN;
                    got_byte = true;
                    if !self.first_byte_seen {
                        self.first_byte_seen = true;
                        UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
                    }
                    if self.feed_byte(byte, now_ms) {
                        got_frame = true;
                    }
                }
            }
            self.dma = Some(dma);
        }

        // Bytes arriving but nothing decoding is a different fault from silence, so it has
        // its own status.
        if got_byte && UART_STATUS.load(Ordering::Relaxed) == LinkStatus::InitOk as u8 {
            LinkStatus::DmaDone.set();
        }

        // The ear has stopped sending: let the loudness fall and count the time as quiet.
        if !got_frame && now_ms.wrapping_sub(self.last_update_ms) >= STOPPED_AFTER_MS {
            self.gate.quiet();
            self.last_update_ms = now_ms;
            self.ear_stopped = true;
        }

        // Fade what the patterns see, so they go quiet instead of holding the last frame.
        if self.ear_stopped {
            let keep = (1.0 - dt_ms / STOPPED_FADE_MS).max(0.0);
            self.onsets.fade(keep);
            self.level_norm *= keep;
        }

        self.gate.tick(dt_ms);
    }

    /// Feed one received byte into the frame assembler. Returns true when a complete,
    /// checksum-valid `BandFrame` was decoded and applied.
    fn feed_byte(&mut self, byte: u8, now_ms: u32) -> bool {
        match self.assembler.feed(byte) {
            Fed::Pending => false,
            Fed::Frame(frame) => {
                self.apply_frame(&frame, now_ms);
                true
            }
            Fed::Bad => {
                // Bad checksum (we locked onto a 0xAA inside the data). Drop it; the
                // stream self-resyncs on the next real sync byte.
                UART_FRAMES_BAD.fetch_add(1, Ordering::Relaxed);
                false
            }
        }
    }

    /// Apply a decoded frame: the bands, the beat and drop detectors, the level, and the
    /// loud flag the Auto-mode timer uses.
    fn apply_frame(&mut self, frame: &BandFrame, now_ms: u32) {
        self.onsets.apply(&frame.bands);
        self.beats.update(norm_from_wire(frame.flux), self.onsets.bass(), now_ms);
        self.drops.update(level_from_wire(frame.bass), now_ms);
        self.level_norm = norm_from_wire(frame.level_norm);
        self.gate.frame(level_from_wire(frame.level));
        self.last_update_ms = now_ms;
        self.ear_stopped = false;
        LinkStatus::Receiving.set();
        UART_FRAMES_OK.fetch_add(1, Ordering::Relaxed);
        UART_LAST_DBFS.store(self.gate.dbfs.to_bits(), Ordering::Relaxed);
        UART_LAST_NORM.store(self.level_norm.to_bits(), Ordering::Relaxed);
    }
}

impl DmaRx {
    /// The DMA engine's write position in the RX ring, from the SIZE register's countdown
    /// of bytes left in this pass (SADDR does not read back live on this chip). None if
    /// the readback is out of range.
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
    let diag = crate::diag::Diag::new();
    let iox = IoxHal::new();
    pins::setup_input_pin(&iox, pins::AUDIO_UART_RX_PORT, pins::AUDIO_UART_RX_PIN, IoxFunction::AF1, IoxEnable::Enable);
    UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

    // UART2 may be owned by another process for a moment at boot, so retry for a few
    // seconds. If it never comes up, run with sound-reactive off.
    const MAX_INIT_ATTEMPTS: u32 = 50; // ~5s at 100ms backoff
    let mut attempt = 0u32;
    loop {
        if let Some(dma) = init_uart() {
            if attempt > 0 {
                diag.line(&format!("audio UART init recovered after {} retries", attempt));
            }
            return Some(dma);
        }
        if attempt == 0 {
            diag.line(&format!("audio UART init failed (status {}); retrying", UART_STATUS.load(Ordering::Relaxed)));
        }
        attempt += 1;
        if attempt >= MAX_INIT_ATTEMPTS {
            diag.line(&format!("audio UART init failed after {} attempts; sound-reactive disabled", MAX_INIT_ATTEMPTS));
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
        Err(_) => { LinkStatus::CsrFail.set(); return None; }
    };

    let ifram_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(bao1x_hal::board::APP_UART_IFRAM_ADDR),
        None, 4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => {
            // Give the CSR page back, or each retry would map another.
            xous::syscall::unmap_memory(csr_mem).ok();
            LinkStatus::IframFail.set();
            return None;
        }
    };

    let csr_virt   = csr_mem.as_ptr() as usize;
    let ifram_virt = ifram_mem.as_ptr() as usize;

    let uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, ifram_virt)
    };
    uart.set_baud(EAR_UART_BAUD, PERCLK_HZ);

    // set_baud leaves the UART in poll mode (Setup bit 0x10). Rewrite Setup without that
    // bit, so RX streams into the UDMA engine, with set_baud's disable-then-configure order.
    let clk_counter: u32 = PERCLK_HZ / EAR_UART_BAUD;
    // Safety: Bank::Custom + UartReg::Setup is the Setup register of the mapped UART CSR.
    unsafe {
        let setup = (csr_virt as *mut u32).add(Bank::Custom as usize + UartReg::Setup as usize);
        setup.write_volatile(0);
        setup.write_volatile(0x0306 | (clk_counter << 16));
    }

    // Start a continuous RX transfer over the RX half of the IFRAM block. 0b1 is the HAL's
    // CFG_CONT, which makes the engine wrap forever; udma_enqueue adds its own enable bit.
    // Safety: the slice describes the physical RX region; only its address/len are used.
    unsafe {
        let rx_phys = core::slice::from_raw_parts(
            (bao1x_hal::board::APP_UART_IFRAM_ADDR + RX_DMA_BUF_START) as *const u8,
            RX_DMA_BUF_LEN,
        );
        uart.udma_enqueue(Bank::Rx, rx_phys, 0b1);
    }

    LinkStatus::InitOk.set();
    let dma = DmaRx { csr_virt, ifram_virt, tail: 0 };
    // Start reading from wherever the engine is now, not from index 0.
    let tail = dma.write_pos().unwrap_or(0);
    Some(DmaRx { tail, ..dma })
}
