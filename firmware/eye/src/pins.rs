use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxDir, IoxEnable, IoxFunction, IoSetup, IoxPort};

// LED output
// BIO pin 4 = PB4 -> the data line all 600 LEDs are driven from
//                                                     [schematic: LED DATA_1, U2.7]
// BIO pin 5 = PB5 -> second data line, wired but unused
//                                                     [schematic: LED DATA_2, U2.6]
// Passed to led::ws2812::Ws2812::new().
#[cfg(not(feature = "previewer"))]
pub const LED_BIO_PIN:   u8 = 4;
#[cfg(not(feature = "previewer"))]
#[allow(dead_code)] // second data line is wired on the board but not driven
pub const LED_BIO_PIN_2: u8 = 5;

// Audio UART (ear -> eye)
// UART2 on the DABAO - the only UART exposed on the board. PB14 is its TX and
// is wired but unused; the eye only receives.
pub const AUDIO_UART_RX_PORT: IoxPort = IoxPort::PB;
pub const AUDIO_UART_RX_PIN:  u8      = 13;

// Panel inputs (d-pad and 3-position sound switch) sit on whichever board the user can
// reach. Without the `input-board` feature they are wired straight to eye GPIOs on the
// combined controller board; with it they live on a separate input board that reaches the
// eye over I2C, leaving PC0/PC1/PC2/PC3/PB2/PB3 free. The IR sensor lands on PC8 either way.

#[cfg(not(feature = "input-board"))]
pub use combined_board::*;

#[cfg(not(feature = "input-board"))]
mod combined_board {
    use super::IoxPort;

    // D-pad buttons
    // Active-low: button press pulls pin to GND; external pull-ups on button board
    // hold pins HIGH when unpressed.
    // Confirmed from controller PCB layout (eye DABAO, socket U1 left edge).
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

    // 3-position sound mode switch
    // Two active-HIGH GPIO lines encode switch position (see buttons/gpio.rs for decode).
    // Switch common is tied to +3.3V; each throw has a 10k pull-down, so the selected
    // line reads HIGH and the others read LOW (center = both LOW).
    // SW_A = SOUND REACTIVE ON line, SW_B = SOUND REACTIVE OFF line.
    // Confirmed from controller PCB layout (eye DABAO, socket U2 right edge).
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

    // MCP23008 I2C GPIO expander carrying the d-pad and the sound switch, reached over the
    // input board cable. A0/A1/A2 are tied to GND, giving 7-bit address 0x20. Every input is
    // active-low and rides the expander's internal pull-ups, so the input board carries no
    // pull resistors of its own.
    pub const EXPANDER_ADDR: u8 = 0x20;

    // Which expander bit each input sits on. Bit 7 is spare. The order follows the board:
    // GP7..GP0 run west to east along the expander's south edge, matching the left-to-right
    // order the controls appear in, so the tracks fan out without crossing.
    pub const EXP_BIT_SW_OFF: u8 = 0;
    pub const EXP_BIT_SW_ON:  u8 = 1;
    pub const EXP_BIT_RIGHT:  u8 = 2;
    pub const EXP_BIT_DOWN:   u8 = 3;
    pub const EXP_BIT_CENTER: u8 = 4;
    pub const EXP_BIT_LEFT:   u8 = 5;
    pub const EXP_BIT_UP:     u8 = 6;

    // Expander interrupt line: open-drain and active-low, pulled up at the eye. Asserted
    // whenever an enabled input changes, so the poll loop only spends an I2C transaction
    // when something actually moved.
    pub const EXPANDER_INT_PORT: IoxPort = IoxPort::PC;
    pub const EXPANDER_INT_PIN:  u8      = 7;
}

// IR receiver
// Everlight IRM-H638T/TR2 - demodulated output, idle HIGH, burst LOW.
// Wired to PC8 (eye DABAO, socket U1.7) = BIO bit 24: ports map in order,
// PB0-15 to BIO 0-15 and PC0-15 to BIO 16-31.
pub const IR_BIO_PIN: u8 = 24;

/// Configure a pin as a schmitt-trigger input for the given function: `IoxFunction::Gpio`
/// for buttons, an AFn for a peripheral like UART RX. `pull_up` sets the internal pull-up;
/// disable it on lines with external pull-downs so the two don't form a voltage divider.
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
