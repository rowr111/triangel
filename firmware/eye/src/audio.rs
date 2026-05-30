use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxDir, IoxEnable, IoxFunction, IoSetup, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::Uart;
use bao1x_hal_service::UdmaGlobal;

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::EAR_UART_BAUD;

use crate::pins;

// --- UART init status - written by listen_loop, read by AudioFill for debug display ---
pub const STATUS_PENDING:    u8 = 0;
pub const STATUS_CSR_FAIL:   u8 = 1;
pub const STATUS_IFRAM_FAIL: u8 = 2;
pub const STATUS_INIT_OK:    u8 = 3;
pub const STATUS_DMA_DONE:   u8 = 4;
pub const STATUS_RECEIVING:  u8 = 5;
pub static UART_STATUS:        AtomicU8  = AtomicU8::new(STATUS_PENDING);
pub static UART_FIRST_BYTE:    AtomicU8  = AtomicU8::new(0);
pub static UART_LAST_FRAME_MS: AtomicU32 = AtomicU32::new(0);

struct AudioState {
    mel:            [f32; MEL_BANDS],
    smoothed_level: f32,
    activity:       bool,
    last_update_ms: u32,
}

impl AudioState {
    fn new() -> Self {
        AudioState { mel: [0.0; MEL_BANDS], smoothed_level: 0.0, activity: false, last_update_ms: 0 }
    }
}

#[derive(Clone)]
pub struct AudioReceiver {
    state: Arc<Mutex<AudioState>>,
}

impl AudioReceiver {
    pub fn new() -> Self {
        let receiver = AudioReceiver { state: Arc::new(Mutex::new(AudioState::new())) };
        receiver.spawn_listener();
        receiver
    }

    pub fn smoothed_level(&self) -> f32 {
        self.state.lock().map(|s| s.smoothed_level).unwrap_or(0.0)
    }

    #[allow(dead_code)]
    pub fn current_mel(&self) -> [f32; MEL_BANDS] {
        self.state.lock().map(|s| s.mel).unwrap_or([0.0; MEL_BANDS])
    }

    pub fn is_active(&self) -> bool {
        self.state.lock().map(|s| s.activity).unwrap_or(false)
    }

    /// Called from the render loop - decays level to zero when ear stops sending.
    pub fn tick_decay(&self, now_ms: u32) {
        if let Ok(mut s) = self.state.try_lock() {
            if now_ms.wrapping_sub(s.last_update_ms) >= 200 {
                s.smoothed_level = (s.smoothed_level - 0.05).max(0.0);
                s.last_update_ms = now_ms;
            }
        }
    }

    fn spawn_listener(&self) {
        let state = self.state.clone();
        std::thread::spawn(move || listen_loop(state));
    }
}


fn init_uart() -> Option<Uart> {
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

    let mut uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, ifram_virt)
    };
    uart.set_baud(EAR_UART_BAUD, PERCLK_HZ);
    uart.setup_async_read();

    UART_STATUS.store(STATUS_INIT_OK, Ordering::Relaxed);
    Some(uart)
}

fn listen_loop(state: Arc<Mutex<AudioState>>) {
    let tt = ticktimer::Ticktimer::new().unwrap();
    let iox = IoxHal::new();
    iox.setup_pin(
        pins::AUDIO_UART_RX_PORT,
        pins::AUDIO_UART_RX_PIN,
        Some(IoxDir::Input),
        Some(IoxFunction::AF1),
        Some(IoxEnable::Enable),
        Some(IoxEnable::Enable),
        None,
        None,
    );
    UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

    let mut uart = match init_uart() {
        Some(u) => u,
        None    => return,
    };

    let mut byte: u8 = 0;
    loop {
        // One byte = the level. That's it.
        while uart.read_async(&mut byte) == 0 {}

        UART_FIRST_BYTE.store(byte, Ordering::Relaxed);
        UART_STATUS.store(STATUS_RECEIVING, Ordering::Relaxed);
        let now = tt.elapsed_ms() as u32;
        UART_LAST_FRAME_MS.store(now, Ordering::Relaxed);

        let level = byte as f32 / 255.0;
        let mut s = state.lock().unwrap();
        // Light EMA so single rogue bytes don't spike the fill
        s.smoothed_level = s.smoothed_level * 0.6 + level * 0.4;
        s.activity       = level > 0.02;
        s.last_update_ms = now;
    }
}
