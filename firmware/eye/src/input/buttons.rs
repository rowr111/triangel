use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bao1x_api::iox::IoxHal;
use bao1x_api::{IoxEnable, IoxFunction, IoxPort, IoxValue};

use super::{EventQueue, InputEvent};
use crate::pins;
use crate::setlist::SoundMode;

// D-pad button inputs are active-low: button press connects pin to GND; pull-up resistors
// on the button board keep them HIGH when unpressed.

// 3-position sound switch is active-HIGH: the common terminal is tied to +3.3V and each
// throw has a 10k pull-down, so the selected line reads HIGH and unselected lines read LOW.
// SW_A = SOUND REACTIVE ON line, SW_B = SOUND REACTIVE OFF line:
//   SW_A LOW,  SW_B HIGH -> SoundMode::Off    (switch in OFF position)
//   SW_A LOW,  SW_B LOW  -> SoundMode::Auto   (center - neither throw connected)
//   SW_A HIGH, SW_B LOW  -> SoundMode::On     (switch in ON position)
// (SW_A HIGH, SW_B HIGH cannot occur: the common feeds only one throw at a time.)

const POLL_MS: usize = 20;
const DEBOUNCE_TICKS: u8 = 3; // consecutive matching reads required to confirm a transition

/// Spawn the button/switch polling thread.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || {
        poll_loop(queue);
    });
}

/// Software debouncer for a single active-low button.
/// Returns true exactly once per confirmed press (falling edge after debounce).
struct Debouncer {
    confirmed: bool, // last stable state (true = HIGH = unpressed)
    candidate: bool, // value accumulating toward a new confirmed state
    count:     u8,
}

impl Debouncer {
    const fn new() -> Self {
        Debouncer { confirmed: true, candidate: true, count: 0 }
    }

    fn update(&mut self, raw: bool) -> bool {
        if raw != self.candidate {
            // Different from what we've been counting - restart
            self.candidate = raw;
            self.count = 1;
        } else if raw != self.confirmed {
            // Same candidate, different from confirmed - keep accumulating
            self.count += 1;
            if self.count >= DEBOUNCE_TICKS {
                let was_high = self.confirmed;
                self.confirmed = raw;
                self.count = 0;
                return was_high && !raw; // falling edge = button pressed (active-low)
            }
        }
        false
    }
}

fn read_switch_position(iox: &IoxHal) -> SoundMode {
    let a = read_pin(iox, pins::SW_A_PORT, pins::SW_A_PIN);
    let b = read_pin(iox, pins::SW_B_PORT, pins::SW_B_PIN);
    match (a, b) {
        (false, true)  => SoundMode::Off,
        (false, false) => SoundMode::Auto, // center: neither throw connected
        (true,  false) => SoundMode::On,
        (true,  true)  => SoundMode::Auto, // can't occur (common feeds one throw); default to center
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

fn poll_loop(queue: Arc<Mutex<VecDeque<InputEvent>>>) {
    let tt  = ticktimer::Ticktimer::new().unwrap();
    let iox = IoxHal::new();
    setup_pins(&iox);

    let mut db_up     = Debouncer::new();
    let mut db_down   = Debouncer::new();
    let mut db_left   = Debouncer::new();
    let mut db_right  = Debouncer::new();
    let mut db_center = Debouncer::new();

    // Sync the boot-time switch position: SetlistManager defaults to Off, and the loop
    // below only fires on changes, so without this the switch is ignored until moved.
    let mut last_switch = read_switch_position(&iox);
    super::lock_queue(&queue).push_back(InputEvent::SetSoundMode(last_switch));

    loop {
        let buttons: [(&mut Debouncer, InputEvent, IoxPort, u8); 5] = [
            (&mut db_up,     InputEvent::BrightnessUp,   pins::BTN_UP_PORT,     pins::BTN_UP_PIN),
            (&mut db_down,   InputEvent::BrightnessDown, pins::BTN_DOWN_PORT,   pins::BTN_DOWN_PIN),
            (&mut db_left,   InputEvent::PatternPrev,    pins::BTN_LEFT_PORT,   pins::BTN_LEFT_PIN),
            (&mut db_right,  InputEvent::PatternNext,    pins::BTN_RIGHT_PORT,  pins::BTN_RIGHT_PIN),
            (&mut db_center, InputEvent::ToggleHold,     pins::BTN_CENTER_PORT, pins::BTN_CENTER_PIN),
        ];
        let mut fired: Vec<InputEvent> = Vec::new();
        for (db, event, port, pin) in buttons {
            if db.update(read_pin(&iox, port, pin)) {
                fired.push(event);
            }
        }

        let sw = read_switch_position(&iox);
        if sw != last_switch {
            last_switch = sw;
            fired.push(InputEvent::SetSoundMode(sw));
        }

        if !fired.is_empty() {
            let mut q = super::lock_queue(&queue);
            for ev in fired {
                q.push_back(ev);
            }
        }

        tt.sleep_ms(POLL_MS).ok();
    }
}

/// Read a GPIO pin. Returns true if HIGH, false if LOW.
fn read_pin(iox: &IoxHal, port: IoxPort, pin: u8) -> bool {
    iox.get_gpio_pin_value(port, pin) == IoxValue::High
}
