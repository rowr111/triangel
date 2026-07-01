pub mod buttons;
pub mod ir;
#[cfg(feature = "previewer")]
pub mod previewer;

use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicBool, Ordering};
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
    CycleSoundMode, // gear button: Off -> Auto -> On -> Off -> ...
}

/// Shared event queue written by input threads, drained by the render loop.
pub type EventQueue = Arc<Mutex<VecDeque<InputEvent>>>;

pub fn new_queue() -> EventQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

static QUEUE_POISON_LOGGED: AtomicBool = AtomicBool::new(false);

/// Lock the shared input queue, recovering (logged once) if a panicking thread poisoned it,
/// so input keeps flowing instead of silently dying for the rest of the run.
pub(crate) fn lock_queue(queue: &EventQueue) -> MutexGuard<'_, VecDeque<InputEvent>> {
    queue.lock().unwrap_or_else(|poisoned| {
        if !QUEUE_POISON_LOGGED.swap(true, Ordering::Relaxed) {
            log::warn!("input event queue poisoned; recovering");
        }
        poisoned.into_inner()
    })
}

/// Drain all pending events and apply them to the setlist manager.
pub fn apply_events(queue: &EventQueue, setlist: &mut SetlistManager, now_ms: u32, sound_active: bool) {
    let mut q = lock_queue(queue);
    while let Some(event) = q.pop_front() {
        match event {
            InputEvent::BrightnessUp      => setlist.adjust_brightness(0.1),
            InputEvent::BrightnessDown    => setlist.adjust_brightness(-0.1),
            InputEvent::PatternNext       => setlist.step_next(now_ms, sound_active),
            InputEvent::PatternPrev       => setlist.step_prev(now_ms, sound_active),
            InputEvent::ToggleHold        => setlist.toggle_hold(),
            InputEvent::SetSoundMode(m)   => setlist.sound_mode = m,
            InputEvent::CycleSoundMode    => setlist.sound_mode = match setlist.sound_mode {
                SoundMode::Off  => SoundMode::Auto,
                SoundMode::Auto => SoundMode::On,
                SoundMode::On   => SoundMode::Off,
            },
        }
    }
}

/// Spawn all input handler threads. They write events into `queue`.
pub fn spawn(queue: EventQueue) {
    buttons::spawn(queue.clone());
    ir::spawn(queue.clone());
    // Previewer builds also accept on-screen d-pad/switch input over USB serial.
    #[cfg(feature = "previewer")]
    previewer::spawn(queue);
}
