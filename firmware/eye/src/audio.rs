use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Bank, DmaReg, Udma, Uart, UartReg};
use bao1x_hal_service::UdmaGlobal;

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::{EAR_UART_BAUD, FRAME_LEN, MelFrame, SYNC_BYTE};

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

// The UART's IFRAM block is split 2048 TX + 2048 RX by the HAL (UART_RX_BUF_START /
// UART_RX_BUF_SIZE in bao1x-hal's uart.rs - private there, so mirrored here). The RX
// half is our DMA ring: the UDMA engine writes incoming bytes into it continuously
// (CFG_CONT wraps forever) and update() chases its write pointer once per render
// frame. Reception is entirely hardware-side: a CPU-serviced byte interface cannot
// keep up with 53-byte bursts at 1 Mbaud (10 us/byte) under a multitasking OS.
const RX_DMA_BUF_START: usize = 2048;
const RX_DMA_BUF_LEN:   usize = 2048;

// --- Auto-mode activity detection - all three are tune-at-bringup placeholders ---
// A level byte above this bar counts as "loud" (the ear currently sends RMS * 1.8).
const ACTIVITY_LOUD_LEVEL: f32 = 0.15;
// Net loud time before reactive mode engages, and unbroken quiet time before it
// releases. Fill is 1:1 with loud time; drain is scaled by ARM/RELEASE, so brief
// quiet gaps (beat spacing, EDM breakdowns) only pause progress, never reset it.
const ACTIVITY_ARM_MS:     f32 = 30_000.0;
const ACTIVITY_RELEASE_MS: f32 = 30_000.0;

/// Custom cache-flush instruction (from the baochip dma_basic2 test) so the CPU
/// re-reads the DMA engine's writes instead of a stale cached copy.
#[inline(always)]
fn cache_flush() {
    // Safety: a hint instruction with no memory operands of its own.
    unsafe {
        core::arch::asm!(".word 0x500F", "nop", "nop", "nop", "nop", "nop");
    }
}

struct AudioState {
    mel:            [f32; MEL_BANDS],
    smoothed_level: f32,
    activity:       bool,
    last_loud:      bool, // freshest byte's verdict; persists across byte-less frames
    loud_ms:        f32,  // leaky accumulator of net loud time, 0..=ACTIVITY_ARM_MS
    last_tick_ms:   u32,
    last_update_ms: u32,
    // Frame assembler (owned by the render loop): the partial frame and fill position.
    frame_buf:      [u8; FRAME_LEN],
    frame_pos:      usize,
}

impl AudioState {
    fn new() -> Self {
        AudioState {
            mel:            [0.0; MEL_BANDS],
            smoothed_level: 0.0,
            activity:       false,
            last_loud:      false,
            loud_ms:        0.0,
            last_tick_ms:   0,
            last_update_ms: 0,
            frame_buf:      [0u8; FRAME_LEN],
            frame_pos:      0,
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

    pub fn smoothed_level(&self) -> f32 {
        self.state.smoothed_level
    }

    #[allow(dead_code)]
    pub fn current_mel(&self) -> [f32; MEL_BANDS] {
        self.state.mel
    }

    pub fn is_active(&self) -> bool {
        self.state.activity
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
        if let Some(pos) = self.dma.as_ref().and_then(|d| d.write_pos()) {
            cache_flush();
            let mut tail = self.dma.as_ref().unwrap().tail;
            while tail != pos {
                let byte = self.dma.as_ref().unwrap().read_ring(tail);
                tail = (tail + 1) % RX_DMA_BUF_LEN;
                UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
                if self.feed_byte(byte, now_ms) {
                    got_frame = true;
                }
            }
            self.dma.as_mut().unwrap().tail = tail;
        }

        // No fresh frame for a while: the ear stopped sending - decay toward silence
        // and count the time as quiet.
        if !got_frame && now_ms.wrapping_sub(self.state.last_update_ms) >= 200 {
            self.state.smoothed_level = (self.state.smoothed_level - 0.05).max(0.0);
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
            false
        }
    }

    /// Apply a decoded frame: the 24 bands, the render level (light EMA), and the loud
    /// flag. The frame's own activity flag is available but not consumed yet - the
    /// eye's slow arm/release accumulator judges the level, unchanged from before.
    fn apply_frame(&mut self, frame: &MelFrame, now_ms: u32) {
        for (m, &b) in self.state.mel.iter_mut().zip(frame.bands.iter()) {
            *m = b as f32 / 65535.0;
        }
        let level = frame.level as f32 / 65535.0;
        // Light EMA so a single rogue frame doesn't spike the fill.
        self.state.smoothed_level = self.state.smoothed_level * 0.6 + level * 0.4;
        self.state.last_loud = self.state.smoothed_level > ACTIVITY_LOUD_LEVEL;
        self.state.last_update_ms = now_ms;
        UART_STATUS.store(STATUS_RECEIVING, Ordering::Relaxed);
        UART_LAST_FRAME_MS.store(now_ms, Ordering::Relaxed);
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
