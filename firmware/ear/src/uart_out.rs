//! Sends the ear's audio level to the eye chip over a hardware UART.
//!
//! CURRENT (temporary, bringup): a single raw byte per audio frame = the level scaled to
//! 0-255 (see `send_level`). No sync byte, no checksum, no framing - the eye's `audio.rs`
//! reads bare bytes and treats `byte / 255` as the level. This is a placeholder while the
//! I2S mic and mel pipeline aren't wired up; the ear derives the level from a simple RMS in
//! `main.rs` for now.
//!
//! FUTURE: the real framed protocol is `MelFrame` in `triangel-shared` - 51 bytes per frame
//! (sync + 48 mel bands + 1 activity byte + 1 XOR checksum), sent via `send()` below. It is
//! dead until the mel path lands; at that point decide whether to give the level byte a real
//! framed variant or drop `MelFrame`.
//!
//! Physical connection: ear pin 15 (PB14, UART2 TX) -> eye pin 16 (PB13, UART2 RX) + GND.
//! Baud rate must match `EAR_UART_BAUD` in eye's `audio.rs`.

use bao1x_hal::clocks::PERCLK_HZ;
use bao1x_hal::udma::{Uart, UartChannel};
use triangel_shared::mel::{EAR_UART_BAUD, FRAME_LEN, MelFrame};

/// Owns the UART TX peripheral; sends the level byte today, framed `MelFrame`s later.
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

    /// Send the current audio level (0.0-1.0) as a single raw byte (`level * 255`, clamped).
    /// Temporary placeholder: no framing/sync/checksum - the eye reads bare bytes.
    pub fn send_level(&mut self, level: f32) {
        let v = (level.clamp(0.0, 1.0) * 255.0) as u8;
        self.uart.write(&[v]);
    }

    /// Full mel frame send (kept for future use).
    #[allow(dead_code)]
    pub fn send(&mut self, frame: &MelFrame) {
        let mut buf = [0u8; FRAME_LEN];
        frame.encode(&mut buf);
        self.uart.write(&buf);
    }
}
