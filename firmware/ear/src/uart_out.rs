//! Sends the ear's audio to the eye chip over a hardware UART as a framed `MelFrame`
//! (see `triangel-shared`): 53 bytes total - a sync byte, the 24 bands, the level, an
//! activity flag, and an XOR checksum. The bands carry the filterbank's normalized
//! output and the level carries absolute dBFS; both are produced on every frame.
//!
//! Physical connection: ear pin 15 (PB14, UART2 TX) wires to eye pin 16 (PB13,
//! UART2 RX), plus GND. Baud rate must match `EAR_UART_BAUD` in eye's `audio.rs`.

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoSetup, IoxDir, IoxDriveStrength, IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel};
use bao1x_hal_service::UdmaGlobal;
use triangel_shared::mel::{EAR_UART_BAUD, FRAME_LEN, MelFrame};

/// Owns the UART TX peripheral and sends framed `MelFrame`s to the eye.
pub struct UartOut {
    uart: Uart,
}

impl UartOut {
    /// Initialize the UART TX peripheral.
    ///
    /// UART2 TX is physical pin 15 (PB14) on the DABAO header - the only UART
    /// broken out. Wire to eye board physical pin 16 (PB13, UART2 RX) + GND.
    pub fn new() -> Self {
        // Uart::new only maps the CSR and sets baud/format - it neither muxes the pin nor
        // gates the UDMA clock on, so do both here (the eye does the same on its RX side)
        // or nothing leaves PB14. Values match the DABAO UART2 pins: output, AF1, 4mA.
        let iox = IoxHal::new();
        iox.setup_pin(
            crate::pins::AUDIO_UART_TX_PORT,
            crate::pins::AUDIO_UART_TX_PIN,
            Some(IoxDir::Output),
            Some(IoxFunction::AF1),
            None,
            None,
            Some(IoxEnable::Enable),
            Some(IoxDriveStrength::Drive4mA),
        );
        UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

        // SAFETY: called once at startup before any other UART use on this channel.
        let uart = unsafe { Uart::new(UartChannel::Uart2, EAR_UART_BAUD, PERCLK_HZ) };
        Self { uart }
    }

    /// Encode and send one `MelFrame` (53 bytes) to the eye.
    pub fn send(&mut self, frame: &MelFrame) {
        let mut buf = [0u8; FRAME_LEN];
        frame.encode(&mut buf);
        self.uart.write(&buf);
    }
}
