//! Sends encoded `MelFrame` packets to the eye chip over a hardware UART.
//!
//! The wire format is defined in `triangel-shared`: 51 bytes per frame
//! (sync + 48 bytes mel bands + 1 activity byte + 1 XOR checksum).
//! The eye chip's `audio.rs` receives and decodes these on the other end.
//!
//! Physical connection: ear UART TX pin -> eye UART RX pin (single wire + GND).
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
    /// Which `UartChannel` maps to the ear->eye wire depends on which pins are
    /// broken out on the ear board header - check the schematic and update
    /// `UartChannel::Uart1` below accordingly.
    pub fn new() -> Self {
        // SAFETY: called once at startup before any other UART use on this channel.
        let uart = unsafe { Uart::new(UartChannel::Uart1, EAR_UART_BAUD, PERCLK_HZ) };
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
