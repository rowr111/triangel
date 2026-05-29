use bao1x_api::IoxPort;

// ── LED output ────────────────────────────────────────────────────────────────
// BIO pin 5 = PB5. Passed to bio_lib::ws2812::Ws2812::new().
#[allow(dead_code)]
pub const LED_BIO_PIN: u8 = 5;

// ── Audio UART (ear → eye) ────────────────────────────────────────────────────
// UART2 on the DABAO — the only UART exposed on the board.
// PB13 = UART2_RX (eye receives mel frames from ear chip)
// PB14 = UART2_TX (eye transmits to ear chip — reserved, currently unused)
#[allow(dead_code)]
pub const AUDIO_UART_RX_PORT: IoxPort = IoxPort::PB;
#[allow(dead_code)]
pub const AUDIO_UART_RX_PIN:  u8      = 13;
#[allow(dead_code)]
pub const AUDIO_UART_TX_PORT: IoxPort = IoxPort::PB;
#[allow(dead_code)]
pub const AUDIO_UART_TX_PIN:  u8      = 14;

// ── D-pad buttons ─────────────────────────────────────────────────────────────
// Active-low: button press pulls pin to GND; external pull-ups on button board
// hold pins HIGH when unpressed.
// TODO: confirm port/pin assignments from PCB layout.
pub const BTN_UP_PORT:     IoxPort = IoxPort::PB;
pub const BTN_UP_PIN:      u8      = 2;
pub const BTN_DOWN_PORT:   IoxPort = IoxPort::PB;
pub const BTN_DOWN_PIN:    u8      = 3;
pub const BTN_LEFT_PORT:   IoxPort = IoxPort::PB;
pub const BTN_LEFT_PIN:    u8      = 4;
pub const BTN_RIGHT_PORT:  IoxPort = IoxPort::PC;
pub const BTN_RIGHT_PIN:   u8      = 0;
pub const BTN_CENTER_PORT: IoxPort = IoxPort::PC;
pub const BTN_CENTER_PIN:  u8      = 1;

// ── 3-position sound mode switch ─────────────────────────────────────────────
// Two active-low GPIO lines encode switch position (see buttons.rs for decode).
// TODO: confirm port/pin assignments from PCB layout.
pub const SW_A_PORT: IoxPort = IoxPort::PC;
pub const SW_A_PIN:  u8      = 2;
pub const SW_B_PORT: IoxPort = IoxPort::PC;
pub const SW_B_PIN:  u8      = 3;

// ── IR receiver ──────────────────────────────────────────────────────────────
// Everlight IRM-H638T/TR2 — demodulated output, idle HIGH, burst LOW.
// TODO: confirm port/pin assignment from PCB layout.
pub const IR_PORT: IoxPort = IoxPort::PC;
pub const IR_PIN:  u8      = 7;
