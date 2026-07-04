use std::pin::Pin;
use std::sync::atomic::{AtomicU32, AtomicU8, AtomicUsize, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel, UartIrq};
use bao1x_hal_service::UdmaGlobal;

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::EAR_UART_BAUD;

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

// --- Lock-free slots between the RX interrupt handler and the render loop ---
static UART_LATEST_BYTE: AtomicU8   = AtomicU8::new(0);   // most recent level byte
static UART_RX_SEQ:      AtomicU32  = AtomicU32::new(0);  // bumped per byte; render loop diffs it
static UART_CSR_VIRT:    AtomicUsize = AtomicUsize::new(0); // handler rebuilds its Uart from these
static UART_IFRAM_VIRT:  AtomicUsize = AtomicUsize::new(0);

struct AudioState {
    mel:            [f32; MEL_BANDS],
    smoothed_level: f32,
    activity:       bool,
    last_update_ms: u32,
    last_seq:       u32,
}

impl AudioState {
    fn new() -> Self {
        AudioState { mel: [0.0; MEL_BANDS], smoothed_level: 0.0, activity: false, last_update_ms: 0, last_seq: 0 }
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

    /// Called once per frame from the render loop. Ingests the latest byte the RX
    /// interrupt captured (EMA + activity), else decays toward silence when the ear
    /// stops sending.
    pub fn update(&mut self, now_ms: u32) {
        // Acquire pairs with the handler's Release bump: a fresh seq guarantees a fresh byte.
        let seq = UART_RX_SEQ.load(Ordering::Acquire);
        if seq != self.state.last_seq {
            self.state.last_seq = seq;
            let level = UART_LATEST_BYTE.load(Ordering::Relaxed) as f32 / 255.0;
            // Light EMA so single rogue bytes don't spike the fill
            self.state.smoothed_level = self.state.smoothed_level * 0.6 + level * 0.4;
            self.state.activity       = level > 0.02;
            self.state.last_update_ms = now_ms;
            UART_STATUS.store(STATUS_RECEIVING, Ordering::Relaxed);
            UART_LAST_FRAME_MS.store(now_ms, Ordering::Relaxed);
        } else if now_ms.wrapping_sub(self.state.last_update_ms) >= 200 {
            // ear stopped sending: drop activity so Auto mode falls back to ambient
            self.state.smoothed_level = (self.state.smoothed_level - 0.05).max(0.0);
            self.state.activity       = false;
            self.state.last_update_ms = now_ms;
        }
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
        UART_LATEST_BYTE.store(byte, Ordering::Relaxed);
        UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
        // Release so the reader's Acquire load of the seq sees the byte stores above.
        UART_RX_SEQ.fetch_add(1, Ordering::Release);
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
