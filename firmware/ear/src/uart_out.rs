//! Sends the ear's audio to the eye chip over a hardware UART, always as a framed
//! `MelFrame` (see `triangel-shared`): 53 bytes total - a sync byte, the 24 bands,
//! the level, an activity flag, and an XOR checksum. The frame format is constant
//! regardless of the `mel` feature, so the two chips never disagree on the wire
//! layout; `mel` only decides whether the bands carry real FFT data (`main.rs` with
//! mel) or are zero with just the level filled in (without mel, via
//! `MelFrame::level_only`).
//!
//! Physical connection: ear pin 15 (PB14, UART2 TX) wires to eye pin 16 (PB13,
//! UART2 RX), plus GND. Baud rate must match `EAR_UART_BAUD` in eye's `audio.rs`.

use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel};
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
