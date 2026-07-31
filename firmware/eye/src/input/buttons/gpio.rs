use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, IoxPort, IoxValue};

use super::Inputs;
use crate::pins;

// D-pad button inputs are active-low: button press connects pin to GND; pull-up resistors
// on the button board keep them HIGH when unpressed.

// 3-position sound switch is active-HIGH: the common terminal is tied to +3.3V and each
// throw has a 10k pull-down, so the selected line reads HIGH and unselected lines read LOW.
// SW_A = SOUND REACTIVE ON line, SW_B = SOUND REACTIVE OFF line:
//   SW_A LOW,  SW_B HIGH -> SoundMode::Off    (switch in OFF position)
//   SW_A LOW,  SW_B LOW  -> SoundMode::Auto   (center - neither throw connected)
//   SW_A HIGH, SW_B LOW  -> SoundMode::On     (switch in ON position)
// (SW_A HIGH, SW_B HIGH cannot occur: the common feeds only one throw at a time.)

/// Panel inputs wired straight to eye GPIOs on the combined controller board.
pub struct Source {
    iox: IoxHal,
}

impl Source {
    pub fn new() -> Self {
        let iox = IoxHal::new();
        setup_pins(&iox);
        Source { iox }
    }

    /// Always returns a fresh reading: the pins are local, so there is nothing worth gating.
    pub fn read(&mut self) -> Option<Inputs> {
        Some(Inputs {
            up:     !read_pin(&self.iox, pins::BTN_UP_PORT,     pins::BTN_UP_PIN),
            down:   !read_pin(&self.iox, pins::BTN_DOWN_PORT,   pins::BTN_DOWN_PIN),
            left:   !read_pin(&self.iox, pins::BTN_LEFT_PORT,   pins::BTN_LEFT_PIN),
            right:  !read_pin(&self.iox, pins::BTN_RIGHT_PORT,  pins::BTN_RIGHT_PIN),
            center: !read_pin(&self.iox, pins::BTN_CENTER_PORT, pins::BTN_CENTER_PIN),
            sw_on:   read_pin(&self.iox, pins::SW_A_PORT,       pins::SW_A_PIN),
            sw_off:  read_pin(&self.iox, pins::SW_B_PORT,       pins::SW_B_PIN),
        })
    }
}

fn setup_pins(iox: &IoxHal) {
    // D-pad buttons: active-low with external pull-ups on the button board; the internal
    // pull-up adds robustness.
    let btn_pins = [
        (pins::BTN_UP_PORT,     pins::BTN_UP_PIN),
        (pins::BTN_DOWN_PORT,   pins::BTN_DOWN_PIN),
        (pins::BTN_LEFT_PORT,   pins::BTN_LEFT_PIN),
        (pins::BTN_RIGHT_PORT,  pins::BTN_RIGHT_PIN),
        (pins::BTN_CENTER_PORT, pins::BTN_CENTER_PIN),
    ];
    for (port, pin) in btn_pins {
        crate::pins::setup_input_pin(iox, port, pin, IoxFunction::Gpio, IoxEnable::Enable);
    }

    // Switch lines: active-high with external 10k pull-downs, so the internal pull-up must
    // stay off - against the pull-down it would form a divider that could misread as HIGH.
    let sw_pins = [
        (pins::SW_A_PORT, pins::SW_A_PIN),
        (pins::SW_B_PORT, pins::SW_B_PIN),
    ];
    for (port, pin) in sw_pins {
        crate::pins::setup_input_pin(iox, port, pin, IoxFunction::Gpio, IoxEnable::Disable);
    }
}

/// Read a GPIO pin. Returns true if HIGH, false if LOW.
fn read_pin(iox: &IoxHal, port: IoxPort, pin: u8) -> bool {
    iox.get_gpio_pin_value(port, pin) == IoxValue::High
}
