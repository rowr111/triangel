pub mod buttons;
pub mod ir;
pub mod nec_capture;
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

/// Drain all pending events and apply them to the setlist manager. `activity` is the
/// audio activity flag; sound_active is derived per event so a mode change earlier in
/// the batch redirects later steps to the setlist the user is now looking at.
pub fn apply_events(queue: &EventQueue, setlist: &mut SetlistManager, now_ms: u32, activity: bool) {
    let mut q = lock_queue(queue);
    while let Some(event) = q.pop_front() {
        let sound_active = setlist.sound_active(activity);
        match event {
            InputEvent::BrightnessUp      => setlist.adjust_brightness(1),
            InputEvent::BrightnessDown    => setlist.adjust_brightness(-1),
            InputEvent::PatternNext       => setlist.step_next(now_ms, sound_active),
            InputEvent::PatternPrev       => setlist.step_prev(now_ms, sound_active),
            InputEvent::ToggleHold        => setlist.toggle_hold(now_ms),
            InputEvent::SetSoundMode(m)   => setlist.sound_mode = m,
            InputEvent::CycleSoundMode    => setlist.sound_mode = setlist.sound_mode.next(),
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
