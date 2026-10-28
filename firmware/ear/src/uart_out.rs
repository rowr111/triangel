//! Sends each `BandFrame` to the eye over UART2: ear pin 15 (PB14, TX) to eye pin 16
//! (PB13, RX), plus GND.

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoSetup, IoxDir, IoxDriveStrength, IoxEnable, IoxFunction, PeriphId};
use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel};
use bao1x_hal_service::UdmaGlobal;
use triangel_shared::frame::{BandFrame, EAR_UART_BAUD, FRAME_LEN};

use crate::pins::{AUDIO_UART_TX_PIN, AUDIO_UART_TX_PORT};

pub struct UartOut {
    uart: Uart,
}

impl UartOut {
    pub fn new() -> Self {
        // Uart::new only maps the CSR and sets baud/format - it neither muxes the pin nor
        // gates the UDMA clock on, so do both here (the eye does the same on its RX side)
        // or nothing leaves PB14. Values match the dabao UART2 pins: output, AF1, 4mA.
        let iox = IoxHal::new();
        iox.setup_pin(
            AUDIO_UART_TX_PORT,
            AUDIO_UART_TX_PIN,
            Some(IoxDir::Output),
            Some(IoxFunction::AF1),
            None,
            None,
            Some(IoxEnable::Enable),
            Some(IoxDriveStrength::Drive4mA),
        );
        UdmaGlobal::new().udma_clock_config(PeriphId::Uart2, true);

        // SAFETY: the UART2 clock was turned on by udma_clock_config above.
        let uart = unsafe { Uart::new(UartChannel::Uart2, EAR_UART_BAUD, PERCLK_HZ) };
        Self { uart }
    }

    /// Encode and send one `BandFrame` to the eye.
    pub fn send(&mut self, frame: &BandFrame) {
        let mut buf = [0u8; FRAME_LEN];
        frame.encode(&mut buf);
        self.uart.write(&buf);
    }
}
