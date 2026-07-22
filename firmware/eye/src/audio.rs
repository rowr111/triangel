use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel, UartIrq};
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

// --- Lock-free byte ring between the RX interrupt (producer) and the render loop
// (consumer): the handler pushes raw bytes, update() drains them through the frame
// state machine. A ring, not a single latest-byte slot, so a 53-byte frame that
// spans several render frames isn't lost. ---
const RX_RING_SZ: usize = 512; // power of two; several frames of headroom
static RX_RING: [AtomicU8; RX_RING_SZ] = [const { AtomicU8::new(0) }; RX_RING_SZ];
static RX_WR: AtomicUsize = AtomicUsize::new(0); // producer index (interrupt)
static RX_RD: AtomicUsize = AtomicUsize::new(0); // consumer index (render loop)
static UART_CSR_VIRT:    AtomicUsize = AtomicUsize::new(0); // handler rebuilds its Uart from these
static UART_IFRAM_VIRT:  AtomicUsize = AtomicUsize::new(0);

// --- Auto-mode activity detection - all three are tune-at-bringup placeholders ---
// A level byte above this bar counts as "loud" (the ear currently sends RMS * 1.8).
const ACTIVITY_LOUD_LEVEL: f32 = 0.15;
// Net loud time before reactive mode engages, and unbroken quiet time before it
// releases. Fill is 1:1 with loud time; drain is scaled by ARM/RELEASE, so brief
// quiet gaps (beat spacing, EDM breakdowns) only pause progress, never reset it.
const ACTIVITY_ARM_MS:     f32 = 30_000.0;
const ACTIVITY_RELEASE_MS: f32 = 30_000.0;

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

pub struct AudioReceiver {
    // Owned and mutated only by the render thread. The RX interrupt hands bytes over through
    // the lock-free atomics above, not through this struct.
    state: AudioState,
    // Parks the IRQ handler registration for the life of the program: the handler
    // dereferences this object's heap location, so it must never move or drop.
    #[allow(dead_code)]
    _uart_irq: Option<Pin<Box<UartIrq>>>,
}

impl Default for AudioReceiver {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioReceiver {
    pub fn new() -> Self {
        AudioReceiver {
            state:     AudioState::new(),
            _uart_irq: init_audio_uart(),
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

    /// Called once per frame from the render loop. Drains the bytes the RX interrupt
    /// buffered through the frame assembler, applies any complete frame, decays toward
    /// silence when the ear stops sending, and advances the slow arm/release accumulator.
    pub fn update(&mut self, now_ms: u32) {
        // Frame delta for the accumulator, capped so boot delay or a frame overrun
        // can't slam it forward in one step.
        let dt_ms = (now_ms.wrapping_sub(self.state.last_tick_ms) as f32).min(100.0);
        self.state.last_tick_ms = now_ms;

        // Drain every byte the interrupt buffered through the frame state machine.
        let mut got_frame = false;
        loop {
            let rd = RX_RD.load(Ordering::Relaxed);
            // Acquire pairs with the handler's Release store of the write index.
            let wr = RX_WR.load(Ordering::Acquire);
            if rd == wr {
                break;
            }
            let byte = RX_RING[rd & (RX_RING_SZ - 1)].load(Ordering::Relaxed);
            RX_RD.store(rd.wrapping_add(1), Ordering::Release);
            if self.feed_byte(byte, now_ms) {
                got_frame = true;
            }
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

/// Bare RX interrupt handler. Runs in restricted context - no allocation, no locks.
/// Rebuilds a Uart handle from the addresses published by init_uart and drains all
/// available bytes into the lock-free slots; the render loop's update() consumes them.
fn audio_uart_handler(_irq_no: usize, _arg: *mut usize) {
    // Acquire pairs with init_uart's Release store of the CSR sentinel.
    let csr_virt = UART_CSR_VIRT.load(Ordering::Acquire);
    if csr_virt == 0 {
        return; // not initialized yet
    }
    let mut uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, UART_IFRAM_VIRT.load(Ordering::Relaxed))
    };
    let mut byte: u8 = 0;
    // Drain so a byte never strands if two arrive between interrupts.
    while uart.read_async(&mut byte) != 0 {
        UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
        // Push into the lock-free ring for update() to frame. Drop on overflow (the
        // render loop fell more than RX_RING_SZ bytes behind - a lost frame, recovers).
        let wr = RX_WR.load(Ordering::Relaxed);
        let rd = RX_RD.load(Ordering::Acquire);
        if wr.wrapping_sub(rd) < RX_RING_SZ {
            RX_RING[wr & (RX_RING_SZ - 1)].store(byte, Ordering::Relaxed);
            // Release so the reader's Acquire load of the write index sees the byte.
            RX_WR.store(wr.wrapping_add(1), Ordering::Release);
        }
    }
}

/// Maps + primes UART2 RX and registers the per-byte interrupt handler. Returns the
/// pinned UartIrq to park for the program's life, or None if the UART never came up
/// (sound-reactive then stays disabled; the LED loop is unaffected).
fn init_audio_uart() -> Option<Pin<Box<UartIrq>>> {
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
        if init_uart() {
            if attempt > 0 {
                log::info!("audio UART init recovered after {} retries", attempt);
            }
            break;
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

    // RX is up and primed: register the per-byte interrupt. Box::pin keeps the handler's
    // heap address stable forever (the IRQ dereferences it). Gate the channel off while
    // registering, then enable. claim_interrupt panics if the IRQ is already held - we
    // own UART2 in this image, so that panic would flag a boot-time conflict to fix.
    let mut uart_irq = Box::pin(UartIrq::new());
    uart_irq.rx_irq_ena(UartChannel::Uart2, false);
    // Safety: uart_irq is returned and parked in AudioReceiver, so it lives forever and never moves.
    unsafe {
        Pin::as_mut(&mut uart_irq).register_handler(UartChannel::Uart2, audio_uart_handler);
    }
    uart_irq.rx_irq_ena(UartChannel::Uart2, true);
    Some(uart_irq)
}

/// Maps and primes UART2 for async RX, publishing the mapped addresses for the handler.
/// Returns false (with UART_STATUS set) if either mapping fails.
fn init_uart() -> bool {
    let csr_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(utralib::utra::udma_uart_2::HW_UDMA_UART_2_BASE),
        None, 4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_CSR_FAIL, Ordering::Relaxed); return false; }
    };

    let ifram_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(bao1x_hal::board::APP_UART_IFRAM_ADDR),
        None, 4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_IFRAM_FAIL, Ordering::Relaxed); return false; }
    };

    let csr_virt   = csr_mem.as_ptr() as usize;
    let ifram_virt = ifram_mem.as_ptr() as usize;
    let _ = (csr_mem, ifram_mem);

    let mut uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, ifram_virt)
    };
    uart.set_baud(EAR_UART_BAUD, PERCLK_HZ);
    uart.setup_async_read();

    // Publish addresses for the handler. CSR_VIRT is its "initialized" sentinel (it bails
    // while it reads 0), so store IFRAM first and CSR last with Release - the handler's
    // Acquire load then guarantees IFRAM is visible before it acts on a non-zero CSR.
    UART_IFRAM_VIRT.store(ifram_virt, Ordering::Relaxed);
    UART_CSR_VIRT.store(csr_virt, Ordering::Release);
    UART_STATUS.store(STATUS_INIT_OK, Ordering::Relaxed);
    true
}
