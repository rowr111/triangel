pub mod buttons;
pub mod ir;
pub mod nec_capture;
pub mod nec_rx;
#[cfg(feature = "previewer")]
pub mod previewer;

use std::sync::mpsc::{self, Receiver, Sender};

use crate::setlist::{SetlistManager, SoundMode, Step};

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

/// The input threads' end of the event channel; the render loop holds the receiver.
pub type EventSender = Sender<InputEvent>;

pub fn channel() -> (EventSender, Receiver<InputEvent>) {
    mpsc::channel()
}

/// Drain all pending events and apply them to the setlist manager. `activity` is the
/// audio activity flag; sound_active is derived per event so a mode change earlier in
/// the batch redirects later steps to the setlist the user is now looking at.
pub fn apply_events(events: &Receiver<InputEvent>, setlist: &mut SetlistManager, now_ms: u32, activity: bool) {
    while let Ok(event) = events.try_recv() {
        let sound_active = setlist.sound_active(activity);
        match event {
            InputEvent::BrightnessUp      => setlist.adjust_brightness(1),
            InputEvent::BrightnessDown    => setlist.adjust_brightness(-1),
            InputEvent::PatternNext       => setlist.step(Step::Next, now_ms, sound_active),
            InputEvent::PatternPrev       => setlist.step(Step::Prev, now_ms, sound_active),
            InputEvent::ToggleHold        => setlist.toggle_hold(now_ms),
            InputEvent::SetSoundMode(m)   => setlist.sound_mode = m,
            InputEvent::CycleSoundMode    => setlist.sound_mode = setlist.sound_mode.next(),
        }
    }
}

/// Spawn all input handler threads. They send events through `events`.
pub fn spawn(events: EventSender) {
    buttons::spawn(events.clone());
    ir::spawn(events.clone());
    // Previewer builds also accept on-screen d-pad/switch input over USB serial.
    #[cfg(feature = "previewer")]
    previewer::spawn(events);
}
