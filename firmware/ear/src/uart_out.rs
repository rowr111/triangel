//! Sends encoded `MelFrame` packets to the eye chip over a hardware UART.
//!
//! The wire format is defined in `triangel-shared`: 51 bytes per frame
//! (sync + 48 bytes mel bands + 1 activity byte + 1 XOR checksum).
//! The eye chip's `audio.rs` receives and decodes these on the other end.
//!
//! Physical connection: ear pin 15 (PB14, UART2 TX) -> eye pin 16 (PB13, UART2 RX) + GND.
//! Baud rate must match `EAR_UART_BAUD` in eye's `audio.rs`.

use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel};
use triangel_shared::mel::{EAR_UART_BAUD, FRAME_LEN, MelFrame};

/// Owns the UART TX peripheral and serialises `MelFrame` packets onto the wire.
pub struct UartOut {
    uart: Uart,
}

impl UartOut {
    /// Initialise the UART TX peripheral.
    ///
    /// UART2 TX is physical pin 15 (PB14) on the DABAO header — the only UART
    /// broken out. Wire to eye board physical pin 16 (PB13, UART2 RX) + GND.
    pub fn new() -> Self {
        // SAFETY: called once at startup before any other UART use on this channel.
        let uart = unsafe { Uart::new(UartChannel::Uart2, EAR_UART_BAUD, PERCLK_HZ) };
        Self { uart }
    }

    /// Encode `frame` into 51 wire bytes and transmit synchronously.
    ///
    /// At 921600 baud, 51 bytes takes ~0.5 ms - well within our 33 ms frame
    /// budget, so a simple blocking write is fine.
    pub fn send(&mut self, frame: &MelFrame) {
        let mut buf = [0u8; FRAME_LEN];
        frame.encode(&mut buf);
        self.uart.write(&buf);
    }
}
