pub mod buttons;
pub mod ir;

use std::sync::{Arc, Mutex};
use std::collections::VecDeque;

use crate::setlist::{SetlistManager, SoundMode};

#[derive(Debug, Clone, Copy)]
pub enum InputEvent {
    BrightnessUp,
    BrightnessDown,
    PatternNext,
    PatternPrev,
    ToggleHold,
    SetSoundMode(SoundMode),
}

/// Shared event queue written by input threads, drained by the render loop.
pub type EventQueue = Arc<Mutex<VecDeque<InputEvent>>>;

pub fn new_queue() -> EventQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

/// Drain all pending events and apply them to the setlist manager.
pub fn apply_events(queue: &EventQueue, setlist: &mut SetlistManager, sound_active: bool) {
    if let Ok(mut q) = queue.lock() {
        while let Some(event) = q.pop_front() {
            match event {
                InputEvent::BrightnessUp      => setlist.adjust_brightness(0.1),
                InputEvent::BrightnessDown    => setlist.adjust_brightness(-0.1),
                InputEvent::PatternNext       => setlist.step_next(sound_active),
                InputEvent::PatternPrev       => setlist.step_prev(sound_active),
                InputEvent::ToggleHold        => setlist.toggle_hold(),
                InputEvent::SetSoundMode(m)   => setlist.sound_mode = m,
            }
        }
    }
}

/// Spawn all input handler threads. They write events into `queue`.
pub fn spawn(queue: EventQueue) {
    buttons::spawn(queue.clone());
    ir::spawn(queue);
}
