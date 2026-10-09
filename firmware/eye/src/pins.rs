use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxDir, IoxEnable, IoxFunction, IoSetup, IoxPort};

// LED output
// BIO pin 4 = PB4 -> chain 1 (288 LEDs, enters at board 1)   [schematic: LED DATA_1, U2.7]
// BIO pin 5 = PB5 -> chain 2 (312 LEDs, enters at board 13)  [schematic: LED DATA_2, U2.6]
#[cfg(not(feature = "previewer"))]
pub const LED_BIO_PIN:   u8 = 4;
#[cfg(not(feature = "previewer"))]
pub const LED_BIO_PIN_2: u8 = 5;

// Audio UART from the ear: UART2 RX. Its TX (PB14) is wired but unused.
pub const AUDIO_UART_RX_PORT: IoxPort = IoxPort::PB;
pub const AUDIO_UART_RX_PIN:  u8      = 13;

// D-pad and 3-position sound switch. Without the `input-board` feature they are wired to
// eye GPIOs; with it they sit behind an I2C expander on a separate board. The IR sensor
// is on PC8 either way.

#[cfg(not(feature = "input-board"))]
pub use combined_board::*;

#[cfg(not(feature = "input-board"))]
mod combined_board {
    use super::IoxPort;

    // D-pad buttons, active-low with external pull-ups (eye DABAO, socket U1 left edge).
    pub const BTN_UP_PORT:     IoxPort = IoxPort::PC;
    pub const BTN_UP_PIN:      u8      = 0;
    pub const BTN_DOWN_PORT:   IoxPort = IoxPort::PC;
    pub const BTN_DOWN_PIN:    u8      = 7;
    pub const BTN_LEFT_PORT:   IoxPort = IoxPort::PC;
    pub const BTN_LEFT_PIN:    u8      = 3;
    pub const BTN_RIGHT_PORT:  IoxPort = IoxPort::PC;
    pub const BTN_RIGHT_PIN:   u8      = 1;
    pub const BTN_CENTER_PORT: IoxPort = IoxPort::PC;
    pub const BTN_CENTER_PIN:  u8      = 2;

    // 3-position sound switch (eye DABAO, socket U2 right edge). Its common is tied to +3.3V
    // and each throw has a 10k pull-down, so the selected line reads HIGH and the center
    // position reads both LOW. SW_A is the ON line, SW_B the OFF line.
    pub const SW_A_PORT: IoxPort = IoxPort::PB;
    pub const SW_A_PIN:  u8      = 3;
    pub const SW_B_PORT: IoxPort = IoxPort::PB;
    pub const SW_B_PIN:  u8      = 2;
}

#[cfg(feature = "input-board")]
pub use input_board::*;

#[cfg(feature = "input-board")]
mod input_board {
    use super::IoxPort;

    // MCP23008 I2C expander carrying the d-pad and the sound switch. A0/A1/A2 are tied to
    // GND, giving 7-bit address 0x20. Every input is active-low on its internal pull-ups.
    pub const EXPANDER_ADDR: u8 = 0x20;

    // The expander bit of each input. Bit 7 is spare.
    pub const EXP_BIT_SW_OFF: u8 = 0;
    pub const EXP_BIT_SW_ON:  u8 = 1;
    pub const EXP_BIT_RIGHT:  u8 = 2;
    pub const EXP_BIT_DOWN:   u8 = 3;
    pub const EXP_BIT_CENTER: u8 = 4;
    pub const EXP_BIT_LEFT:   u8 = 5;
    pub const EXP_BIT_UP:     u8 = 6;

    // Expander interrupt line: open-drain and active-low, pulled up at the eye. It asserts
    // when an input changes.
    pub const EXPANDER_INT_PORT: IoxPort = IoxPort::PC;
    pub const EXPANDER_INT_PIN:  u8      = 7;
}

// IR receiver (Everlight IRM-H638T/TR2): idle HIGH, burst LOW. It is on PC8 (socket U1.7),
// which is BIO bit 24: PB0-15 are BIO 0-15 and PC0-15 are BIO 16-31.
pub const IR_BIO_PIN: u8 = 24;

/// Configure a pin as a schmitt-trigger input for `function`. Turn `pull_up` off on lines
/// with external pull-downs, so the two do not form a voltage divider.
pub fn setup_input_pin(iox: &IoxHal, port: IoxPort, pin: u8, function: IoxFunction, pull_up: IoxEnable) {
    iox.setup_pin(
        port,
        pin,
        Some(IoxDir::Input),
        Some(function),
        Some(IoxEnable::Enable), // schmitt trigger
        Some(pull_up),
        None,
        None,
    );
}
