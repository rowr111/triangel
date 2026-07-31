use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use super::{EventQueue, InputEvent};
use crate::setlist::SoundMode;

// Where the seven panel inputs are read from. The source module owns the wiring details -
// which pins or which expander bits, and which polarity - and hands back normalized state.
#[cfg(not(feature = "input-board"))]
#[path = "buttons/gpio.rs"]
mod source;

#[cfg(feature = "input-board")]
#[path = "buttons/expander.rs"]
mod source;

const POLL_MS: usize = 20;
const DEBOUNCE_TICKS: u8 = 3; // consecutive matching reads required to confirm a transition

/// State of the panel inputs, normalized by the source module so `true` means pressed for a
/// button and selected for a switch throw, whichever polarity the board happens to wire.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Inputs {
    pub up:     bool,
    pub down:   bool,
    pub left:   bool,
    pub right:  bool,
    pub center: bool,
    pub sw_on:  bool,
    pub sw_off: bool,
}

/// Spawn the button/switch polling thread.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || {
        poll_loop(queue);
    });
}

/// Software debouncer for a single button.
/// Returns true exactly once per confirmed press (rising edge after debounce).
struct Debouncer {
    confirmed: bool, // last stable state (true = pressed)
    candidate: bool, // value accumulating toward a new confirmed state
    count:     u8,
}

impl Debouncer {
    const fn new() -> Self {
        Debouncer { confirmed: false, candidate: false, count: 0 }
    }

    fn update(&mut self, pressed: bool) -> bool {
        if pressed != self.candidate {
            // Different from what we've been counting - restart
            self.candidate = pressed;
            self.count = 1;
        } else if pressed != self.confirmed {
            // Same candidate, different from confirmed - keep accumulating
            self.count += 1;
            if self.count >= DEBOUNCE_TICKS {
                let was_released = !self.confirmed;
                self.confirmed = pressed;
                self.count = 0;
                return was_released && pressed; // rising edge = button pressed
            }
        }
        false
    }
}

/// Decode the two switch throws. Only one can be selected at a time; neither = center.
fn switch_position(inputs: &Inputs) -> SoundMode {
    match (inputs.sw_on, inputs.sw_off) {
        (true,  false) => SoundMode::On,
        (false, true)  => SoundMode::Off,
        (false, false) => SoundMode::Auto, // center: neither throw connected
        (true,  true)  => SoundMode::Auto, // can't occur (common feeds one throw); default to center
    }
}

fn poll_loop(queue: Arc<Mutex<VecDeque<InputEvent>>>) {
    let tt = ticktimer::Ticktimer::new().unwrap();
    let mut source = source::Source::new();

    let mut db_up     = Debouncer::new();
    let mut db_down   = Debouncer::new();
    let mut db_left   = Debouncer::new();
    let mut db_right  = Debouncer::new();
    let mut db_center = Debouncer::new();

    // Last reading, held across ticks: a source that only reports on change leaves most
    // ticks with nothing new, and the debouncer still needs a value every tick.
    let mut inputs: Option<Inputs> = None;
    // Sync the switch position on the first reading: SetlistManager defaults to Off, and the
    // loop below only fires on changes, so without this the switch is ignored until moved.
    let mut last_switch: Option<SoundMode> = None;

    loop {
        if let Some(fresh) = source.read() {
            inputs = Some(fresh);
        }

        if let Some(inputs) = inputs {
            let buttons: [(&mut Debouncer, InputEvent, bool); 5] = [
                (&mut db_up,     InputEvent::BrightnessUp,   inputs.up),
                (&mut db_down,   InputEvent::BrightnessDown, inputs.down),
                (&mut db_left,   InputEvent::PatternPrev,    inputs.left),
                (&mut db_right,  InputEvent::PatternNext,    inputs.right),
                (&mut db_center, InputEvent::ToggleHold,     inputs.center),
            ];
            let mut fired: Vec<InputEvent> = Vec::new();
            for (db, event, pressed) in buttons {
                if db.update(pressed) {
                    fired.push(event);
                }
            }

            let sw = switch_position(&inputs);
            if last_switch != Some(sw) {
                last_switch = Some(sw);
                fired.push(InputEvent::SetSoundMode(sw));
            }

            if !fired.is_empty() {
                let mut q = super::lock_queue(&queue);
                for ev in fired {
                    q.push_back(ev);
                }
            }
        }

        tt.sleep_ms(POLL_MS).ok();
    }
}
