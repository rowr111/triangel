use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU32, AtomicU8, Ordering};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxDir, IoxEnable, IoxFunction, IoSetup, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::Uart;
use bao1x_hal_service::UdmaGlobal;

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::{EAR_UART_BAUD, FRAME_LEN, MelFrame};

use crate::pins;

// --- UART init status — written by listen_loop, read by AudioFill for debug display ---
pub const STATUS_PENDING:    u8 = 0; // thread started, init not yet attempted
pub const STATUS_CSR_FAIL:   u8 = 1; // map_memory for UART2 registers failed (owned by another process?)
pub const STATUS_IFRAM_FAIL: u8 = 2; // map_memory for pre-reserved IFRAM page failed
pub const STATUS_INIT_OK:    u8 = 3; // init succeeded, waiting for first frame
pub const STATUS_DMA_DONE:   u8 = 4; // uart.read() returned but MelFrame::decode failed (bad sync/checksum)
pub const STATUS_RECEIVING:  u8 = 5; // actively receiving frames from ear chip
pub static UART_STATUS: AtomicU8 = AtomicU8::new(STATUS_PENDING);
/// First byte received after DMA completes — helps diagnose what's on the wire.
pub static UART_FIRST_BYTE: AtomicU8 = AtomicU8::new(0);
/// Timestamp (ms) of last successfully decoded frame — used to detect stale status.
/// Lower 32 bits of the ticktimer ms timestamp of the last decoded frame.
pub static UART_LAST_FRAME_MS: AtomicU32 = AtomicU32::new(0);

struct AudioState {
    mel:            [f32; MEL_BANDS],
    smoothed_level: f32,
    envelope:       f32,
    activity:       bool,
}

impl AudioState {
    fn new() -> Self {
        AudioState { mel: [0.0; MEL_BANDS], smoothed_level: 0.0, envelope: 0.0, activity: false }
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

    /// Smoothed normalised sound level 0.0-1.0 with attack/decay envelope.
    pub fn smoothed_level(&self) -> f32 {
        self.state.lock().map(|s| s.smoothed_level).unwrap_or(0.0)
    }

    /// Raw mel band values, 24 bands, each 0.0-1.0.
    #[allow(dead_code)]
    pub fn current_mel(&self) -> [f32; MEL_BANDS] {
        self.state.lock().map(|s| s.mel).unwrap_or([0.0; MEL_BANDS])
    }

    /// Activity flag: true when sustained absolute sound energy exceeds the ear's
    /// calibrated threshold. Use for Auto sound mode rather than smoothed_level.
    pub fn is_active(&self) -> bool {
        self.state.lock().map(|s| s.activity).unwrap_or(false)
    }

    fn spawn_listener(&self) {
        let state = self.state.clone();
        std::thread::spawn(move || listen_loop(state));
    }
}

const ATTACK: f32 = 0.25;
const DECAY:  f32 = 0.02;

fn apply_envelope(envelope: f32, new_level: f32) -> f32 {
    if new_level > envelope {
        envelope + ATTACK * (new_level - envelope)
    } else {
        (envelope - DECAY).max(new_level).max(0.0)
    }
}

/// Initialise UART2 for receiving mel frames from the ear chip.
///
/// Uses get_handle + explicit map_memory calls instead of Uart::new, so
/// each failure point returns None rather than panicking.
// Offset of the RX buffer within the IFRAM page (matches bao1x_hal udma::uart internals).
fn init_uart() -> Option<Uart> {
    // Map UART2 hardware registers. Fails if another process already owns them.
    let csr_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(utralib::utra::udma_uart_2::HW_UDMA_UART_2_BASE),
        None,
        4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_CSR_FAIL, Ordering::Relaxed); return None; }
    };

    // Map the loader-reserved IFRAM page for app UART (always available, no allocator needed).
    let ifram_mem = match xous::syscall::map_memory(
        xous::MemoryAddress::new(bao1x_hal::board::APP_UART_IFRAM_ADDR),
        None,
        4096,
        xous::MemoryFlags::R | xous::MemoryFlags::W,
    ) {
        Ok(m) => m,
        Err(_) => { UART_STATUS.store(STATUS_IFRAM_FAIL, Ordering::Relaxed); return None; }
    };

    let csr_virt   = csr_mem.as_ptr()   as usize;
    let ifram_virt = ifram_mem.as_ptr() as usize;
    // MemoryRange is Copy with no Drop — mappings persist when these go out of scope.
    let _ = (csr_mem, ifram_mem);

    let mut uart = unsafe {
        Uart::get_handle(csr_virt, bao1x_hal::board::APP_UART_IFRAM_ADDR, ifram_virt)
    };
    // set_baud uses 0x0316 which includes poll mode (bit 4), routing bytes to the
    // command register — exactly what read_async() needs.
    uart.set_baud(EAR_UART_BAUD, PERCLK_HZ);
    // Prime the UART RX path — required before any characters can be received.
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
        Some(IoxEnable::Enable), // schmitt trigger
        Some(IoxEnable::Enable), // pullup
        None,
        None,
    );
    UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

    let mut uart = match init_uart() {
        Some(u) => u,
        None    => return,
    };

    let mut buf = [0u8; FRAME_LEN];
    loop {
        // Signal waiting-for-frame before blocking read so render can detect gaps.
        UART_STATUS.store(STATUS_INIT_OK, Ordering::Relaxed);
        // Read bytes one at a time via poll-mode command register (no DMA needed).
        // Must busy-wait continuously — poll mode holds only one byte at a time,
        // any bytes arriving while we yield are dropped.
        for byte in buf.iter_mut() {
            let mut c = 0u8;
            while uart.read_async(&mut c) == 0 {}
            *byte = c;
        }
        UART_FIRST_BYTE.store(buf[0], Ordering::Relaxed);
        UART_STATUS.store(STATUS_DMA_DONE, Ordering::Relaxed);
        match MelFrame::decode(&buf) {
            Some(frame) => {
                UART_STATUS.store(STATUS_RECEIVING, Ordering::Relaxed);
                UART_LAST_FRAME_MS.store(tt.elapsed_ms() as u32, Ordering::Relaxed);
                let mut s = state.lock().unwrap();
                for (i, &raw) in frame.bands.iter().enumerate() {
                    s.mel[i] = raw as f32 / 65535.0;
                }
                let raw_level = s.mel.iter().copied().fold(0.0_f32, f32::max);
                s.envelope    = apply_envelope(s.envelope, raw_level);
                s.smoothed_level = s.envelope;
                s.activity    = frame.activity;
            }
            None => {
                // bad sync or checksum — discard one byte to re-align with frame boundary
                let mut c = 0u8;
                while uart.read_async(&mut c) == 0 {}
            }
        }
    }
}
