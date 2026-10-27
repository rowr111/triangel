use bao1x_api::IoxPort;

// Audio UART (ear -> eye)
// UART2 on the dabao - the only UART exposed on the board.
// PB14 = UART2_TX (ear transmits band frames to the eye chip)
// Wires to eye physical pin 16 = PB13 = UART2_RX. See uart_out.rs.
// (The UART2 peripheral is selected by UartChannel::Uart2 in uart_out.rs; these set up its
// pin.)
pub const AUDIO_UART_TX_PORT: IoxPort = IoxPort::PB;
pub const AUDIO_UART_TX_PIN: u8 = 14;

// ICS43434 MEMS microphone, I2S through a BIO program. The pins are compiled into that
// program (i2s_bio.rs), so moving one means regenerating it; see
// docs/how-to/regenerate-the-mic-program.md.
// BIO pin number == PB pin number. Avoid PB11/PB12 (used for I2C).
pub const MIC_BCLK_BIO_PIN: u8 = 1; // PB1
pub const MIC_SD_BIO_PIN: u8 = 2; // PB2
pub const MIC_WS_BIO_PIN: u8 = 3; // PB3
