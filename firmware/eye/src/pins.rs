use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxDir, IoxEnable, IoxFunction, IoSetup, IoxPort};

// LED output
// BIO pin 4 = PB4 -> chain 1 (tiles 1-12, 288 LEDs)   [schematic: LED DATA_1, U2.7]
// BIO pin 5 = PB5 -> chain 2 (tiles 13-25, 312 LEDs)  [schematic: LED DATA_2, U2.6]
// Passed to bio_lib::ws2812::Ws2812::new().
#[allow(dead_code)]
pub const LED_BIO_PIN:   u8 = 4;
#[allow(dead_code)]
pub const LED_BIO_PIN_2: u8 = 5;

// Audio UART (ear -> eye)
// UART2 on the DABAO - the only UART exposed on the board.
// PB13 = UART2_RX (eye receives the ear's audio level bytes)
// PB14 = UART2_TX (eye transmits to ear chip - reserved, currently unused)
#[allow(dead_code)]
pub const AUDIO_UART_RX_PORT: IoxPort = IoxPort::PB;
#[allow(dead_code)]
pub const AUDIO_UART_RX_PIN:  u8      = 13;
#[allow(dead_code)]
pub const AUDIO_UART_TX_PORT: IoxPort = IoxPort::PB;
#[allow(dead_code)]
pub const AUDIO_UART_TX_PIN:  u8      = 14;

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
// Two active-HIGH GPIO lines encode switch position (see buttons.rs for decode).
// Switch common is tied to +3.3V; each throw has a 10k pull-down, so the selected
// line reads HIGH and the others read LOW (center = both LOW).
// SW_A = SOUND REACTIVE ON line, SW_B = SOUND REACTIVE OFF line.
// Confirmed from controller PCB layout (eye DABAO, socket U2 right edge).
pub const SW_A_PORT: IoxPort = IoxPort::PB;
pub const SW_A_PIN:  u8      = 3;
pub const SW_B_PORT: IoxPort = IoxPort::PB;
pub const SW_B_PIN:  u8      = 2;

// IR receiver
// Everlight IRM-H638T/TR2 - demodulated output, idle HIGH, burst LOW.
// Confirmed from controller PCB layout (eye DABAO, socket U1.7). The BIO pin
// number for PulseCapture is derived automatically from this port/pin (PC8).
pub const IR_PORT: IoxPort = IoxPort::PC;
pub const IR_PIN:  u8      = 8;

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
