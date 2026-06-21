use bao1x_api::IoxPort;

// Audio UART (ear -> eye)
// UART2 on the DABAO - the only UART exposed on the board.
// PB14 = UART2_TX (ear transmits mel/level frames to the eye chip)
// Wires to eye physical pin 16 = PB13 = UART2_RX. See uart_out.rs.
// (The UART2 peripheral is selected by UartChannel::Uart2 in uart_out.rs; these
// constants document the physical pin for reference.)
#[allow(dead_code)]
pub const AUDIO_UART_TX_PORT: IoxPort = IoxPort::PB;
#[allow(dead_code)]
pub const AUDIO_UART_TX_PIN:  u8      = 14;

// ICS43434 MEMS microphone - I2S, implemented as a BIO driver (not the hardware
// UDMA I2S peripheral), so the pins are not tied to a fixed alternate-function
// table - any BIO pin works. BIO pin number == PB pin number (PB1 = BIO1, etc).
// Avoid PB11/PB12 (used for I2C). Passed to the future BIO I2S driver.
// BCLK = PB1, SD (mic data out) = PB2, WS/LRCLK = PB3.
#[allow(dead_code)]
pub const MIC_BCLK_BIO_PIN: u8 = 1; // PB1
#[allow(dead_code)]
pub const MIC_SD_BIO_PIN:   u8 = 2; // PB2
#[allow(dead_code)]
pub const MIC_WS_BIO_PIN:   u8 = 3; // PB3
