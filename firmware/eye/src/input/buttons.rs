use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use bao1x_api::IoxPort;

use super::{EventQueue, InputEvent};
use crate::pins;
use crate::setlist::SoundMode;

// All button inputs are active-low: button press connects pin to GND; pull-up resistors
// on the button board keep them HIGH when unpressed.

// 3-position switch: two active-low GPIO lines encode position.
// With pull-ups, a floating pin reads HIGH. Switch connects one pin to GND per position:
//   SW_A LOW,  SW_B HIGH -> SoundMode::Off
//   SW_A HIGH, SW_B HIGH -> SoundMode::Auto  (center - neither connected)
//   SW_A HIGH, SW_B LOW  -> SoundMode::On

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

fn read_switch_position() -> SoundMode {
    let a = read_pin(pins::SW_A_PORT, pins::SW_A_PIN); // true = HIGH = not in this switch position
    let b = read_pin(pins::SW_B_PORT, pins::SW_B_PIN);
    match (a, b) {
        (false, true)  => SoundMode::Off,
        (true,  true)  => SoundMode::Auto,
        (true,  false) => SoundMode::On,
        (false, false) => SoundMode::Auto, // both grounded: shouldn't happen, default to center
    }
}

fn poll_loop(queue: Arc<Mutex<VecDeque<InputEvent>>>) {
    let tt = ticktimer::Ticktimer::new().unwrap();

    let mut db_up     = Debouncer::new();
    let mut db_down   = Debouncer::new();
    let mut db_left   = Debouncer::new();
    let mut db_right  = Debouncer::new();
    let mut db_center = Debouncer::new();

    let mut last_switch = read_switch_position();

    loop {
        let mut pending = [None::<InputEvent>; 6];
        let mut n = 0;

        if db_up.update(read_pin(pins::BTN_UP_PORT,     pins::BTN_UP_PIN))     { pending[n] = Some(InputEvent::BrightnessUp);   n += 1; }
        if db_down.update(read_pin(pins::BTN_DOWN_PORT,   pins::BTN_DOWN_PIN))   { pending[n] = Some(InputEvent::BrightnessDown); n += 1; }
        if db_left.update(read_pin(pins::BTN_LEFT_PORT,   pins::BTN_LEFT_PIN))   { pending[n] = Some(InputEvent::PatternPrev);    n += 1; }
        if db_right.update(read_pin(pins::BTN_RIGHT_PORT,  pins::BTN_RIGHT_PIN))  { pending[n] = Some(InputEvent::PatternNext);    n += 1; }
        if db_center.update(read_pin(pins::BTN_CENTER_PORT, pins::BTN_CENTER_PIN)) { pending[n] = Some(InputEvent::ToggleHold);     n += 1; }

        let sw = read_switch_position();
        if sw != last_switch {
            last_switch = sw;
            pending[n] = Some(InputEvent::SetSoundMode(sw));
            n += 1;
        }

        if n > 0 {
            if let Ok(mut q) = queue.lock() {
                for ev in pending.iter().take(n).flatten() {
                    q.push_back(*ev);
                }
            }
        }

        tt.sleep_ms(POLL_MS).ok();
    }
}

/// Read a GPIO pin. Returns true if HIGH, false if LOW.
/// TODO: replace with actual bao1x GPIO HAL call once pin assignments are confirmed.
#[allow(unused_variables)]
fn read_pin(port: IoxPort, pin: u8) -> bool {
    true // stub - HIGH = unpressed (active-low buttons with pull-ups)
}
