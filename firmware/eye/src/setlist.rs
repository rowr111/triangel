use std::collections::VecDeque;

use crate::led::map::LED_COUNT;
use crate::patterns::transition::{self, TransitionStyle};
use crate::audio::Audio;
use crate::patterns::{Frame, Pattern};
use crate::patterns::ambient::{drizzle::Drizzle, effervesce::Effervesce, flame::ApexFlame, fubuki::Fubuki, rainbow::Rainbow, ricochet::Ricochet, shimmer::CenterShimmer, squall::Squall, uzumaki::Uzumaki};
use crate::patterns::reactive::{
    firework::Firework, raindrop::Raindrop, spectrum::Spectrum, spiderweb::Spiderweb, tiles::Tiles,
};

const CYCLE_MS: u32 = 3 * 60 * 1_000; // 3 minutes

// Transition durations: leisurely for manual/auto pattern steps, snappy for the
// ambient<->reactive sound flip so it tracks the music rather than lagging it.
const STEP_TRANSITION_MS:  u32 = 3000;
const SOUND_TRANSITION_MS: u32 = 200;

// A pattern not drawn for longer than this has left the screen, so it gets `on_enter`
// when it is next drawn.
const REENTRY_GAP_MS: u32 = 500;

// Global brightness ladder: geometric steps (~1.7x each) so a d-pad press feels like an
// even perceived change instead of lurching at the low end. Applied as a flat multiply
// over the finished frame; the top level is 1.0 (full brightness, a no-op).
const BRIGHTNESS_LEVELS: [f32; 7] = [0.06, 0.10, 0.17, 0.29, 0.49, 0.83, 1.0];
#[cfg(not(feature = "previewer"))]
const DEFAULT_BRIGHTNESS_INDEX: usize = 3; // middle -> perceptual medium, room to go both ways
#[cfg(feature = "previewer")]
const DEFAULT_BRIGHTNESS_INDEX: usize = BRIGHTNESS_LEVELS.len() - 1; // full - the web view is dim otherwise

// Audition order for pick_style - cycled so each step shows a different transition.
const STYLES: [TransitionStyle; 7] = [
    TransitionStyle::Crossfade,
    TransitionStyle::RadialOut,
    TransitionStyle::RadialIn,
    TransitionStyle::Sparkle,
    TransitionStyle::RadialSparkle,
    TransitionStyle::SpiralOut,
    TransitionStyle::SpiralIn,
];

fn ambient_patterns() -> Vec<Box<dyn Pattern>> {
    vec![
        Box::new(Fubuki::new()),
        Box::new(Squall::new()),
        Box::new(CenterShimmer::new(60.0, 120.0)),
        Box::new(ApexFlame::new(100.0, 80.0)),
        Box::new(Uzumaki::new()),
        Box::new(Effervesce::new()),
        Box::new(Rainbow::new()),
        Box::new(Ricochet::new()),
        Box::new(Drizzle::new()),
    ]
}

fn reactive_patterns() -> Vec<Box<dyn Pattern>> {
    vec![
        Box::new(Tiles::new()),
        Box::new(Spectrum::new()),
        Box::new(Raindrop::new()),
        Box::new(Firework::new()),
        Box::new(Spiderweb::new()),
    ]
}

// --- A single ordered list of patterns with its own cursor ---

struct Setlist {
    patterns: Vec<Box<dyn Pattern>>,
    /// When each pattern was last drawn, if ever.
    drawn_ms: Vec<Option<u32>>,
    idx:      usize,
}

impl Setlist {
    fn new(patterns: Vec<Box<dyn Pattern>>) -> Self {
        let drawn_ms = vec![None; patterns.len()];
        Setlist { patterns, drawn_ms, idx: 0 }
    }

    fn len(&self) -> usize {
        self.patterns.len()
    }

    /// Draw pattern `i`, telling it first if it has been off screen.
    fn draw(&mut self, i: usize, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if self.drawn_ms[i].is_none_or(|last| t_ms.wrapping_sub(last) > REENTRY_GAP_MS) {
            self.patterns[i].on_enter(t_ms);
        }
        self.drawn_ms[i] = Some(t_ms);
        self.patterns[i].render(t_ms, audio, out);
    }

    fn step(&mut self, dir: Step) {
        let len = self.patterns.len();
        self.idx = match dir {
            Step::Next => (self.idx + 1) % len,
            Step::Prev => (self.idx + len - 1) % len,
        };
    }
}

// --- Transitions ---

/// Which setlist; also its index in `SetlistManager::setlists`.
#[derive(Clone, Copy, PartialEq)]
enum SetlistKind {
    Ambient = 0,
    Reactive = 1,
}

impl SetlistKind {
    fn from_sound(sound_active: bool) -> Self {
        if sound_active { SetlistKind::Reactive } else { SetlistKind::Ambient }
    }
}

/// A transition in flight. Stores only what we're fading *from*; the current pattern is
/// always the live target, so it never needs recording.
#[derive(Clone, Copy)]
struct Transition {
    from_kind:   SetlistKind,
    from_idx:    usize,
    start_ms:    u32,
    duration_ms: u32,
    style:       TransitionStyle,
}

#[derive(Clone, Copy)]
pub enum Step {
    Next,
    Prev,
}

// --- Sound mode ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundMode {
    Off,
    Auto,
    On,
}

impl SoundMode {
    /// Cycle Off -> Auto -> On -> Off (the gear button).
    pub fn next(self) -> Self {
        match self {
            SoundMode::Off  => SoundMode::Auto,
            SoundMode::Auto => SoundMode::On,
            SoundMode::On   => SoundMode::Off,
        }
    }
}

// --- Setlist manager ---

pub struct SetlistManager {
    setlists:          [Setlist; 2],
    last_cycle_ms:     u32,
    held:              bool,
    transition:        Option<Transition>,
    pending:           VecDeque<Step>,
    last_sound_active: bool,
    next_style:        usize,
    from_buf:          Frame,
    brightness_index:  usize,
    pub sound_mode:    SoundMode,
}

impl SetlistManager {
    pub fn new(now_ms: u32) -> Self {
        SetlistManager {
            setlists:          [
                Setlist::new(ambient_patterns()),
                Setlist::new(reactive_patterns()),
            ],
            last_cycle_ms:     now_ms,
            held:              false,
            transition:        None,
            pending:           VecDeque::new(),
            last_sound_active: false,
            next_style:        0,
            from_buf:          [[0u8; 3]; LED_COUNT],
            brightness_index:  DEFAULT_BRIGHTNESS_INDEX,
            sound_mode:        SoundMode::Off,
        }
    }

    fn list(&mut self, kind: SetlistKind) -> &mut Setlist {
        &mut self.setlists[kind as usize]
    }

    /// Cycle through the transition styles so each new step shows a different one - handy
    /// for auditioning in the previewer. Swap for a fixed pick or randomise here later.
    fn pick_style(&mut self) -> TransitionStyle {
        let style = STYLES[self.next_style % STYLES.len()];
        self.next_style = self.next_style.wrapping_add(1);
        style
    }

    fn begin_transition(&mut self, from_kind: SetlistKind, from_idx: usize, now_ms: u32, duration_ms: u32, style: TransitionStyle) {
        self.transition = Some(Transition { from_kind, from_idx, start_ms: now_ms, duration_ms, style });
    }

    /// Render the current frame, compositing an in-flight transition over it. Detects the
    /// ambient<->reactive flip and starts a fast crossfade for it.
    pub fn render(&mut self, t_ms: u32, audio: &Audio, sound_active: bool, out: &mut Frame) {
        // Sound flip is a hard setlist change: it jumps the queue (drops pending steps that
        // belong to the old setlist) and crossfades immediately.
        if sound_active != self.last_sound_active {
            let from_kind = SetlistKind::from_sound(self.last_sound_active);
            let from_idx  = self.list(from_kind).idx;
            self.pending.clear();
            self.begin_transition(from_kind, from_idx, t_ms, SOUND_TRANSITION_MS, TransitionStyle::Crossfade);
            self.last_sound_active = sound_active;
        }

        // Retire a finished transition and start the next queued step from the pattern that
        // just finished fading in - seamless (the new fade begins at progress 0 == that pattern).
        if let Some(tr) = self.transition {
            if t_ms.wrapping_sub(tr.start_ms) >= tr.duration_ms {
                self.transition = None;
                self.pump_queue(SetlistKind::from_sound(sound_active), t_ms);
            }
        }

        // Live current pattern -> out.
        let to_kind = SetlistKind::from_sound(sound_active);
        let to_idx  = self.list(to_kind).idx;
        self.setlists[to_kind as usize].draw(to_idx, t_ms, audio, out);

        // Composite the outgoing pattern over it while a transition is running.
        if let Some(tr) = self.transition {
            // max(1) guards against a zero-duration transition dividing by zero.
            let progress = t_ms.wrapping_sub(tr.start_ms) as f32 / tr.duration_ms.max(1) as f32;
            self.setlists[tr.from_kind as usize].draw(tr.from_idx, t_ms, audio, &mut self.from_buf);
            transition::blend(tr.style, progress, &self.from_buf, out);
        }
    }

    /// Call once per frame. Advances pattern index when the cycle timer expires.
    pub fn tick(&mut self, t_ms: u32, sound_active: bool) {
        if self.held {
            return;
        }
        if t_ms.wrapping_sub(self.last_cycle_ms) >= CYCLE_MS {
            self.enqueue_step(SetlistKind::from_sound(sound_active), Step::Next, t_ms);
        }
    }

    /// Step one pattern in the setlist on screen. `now_ms` restarts the cycle countdown so
    /// a manual step doesn't immediately auto-advance.
    pub fn step(&mut self, dir: Step, now_ms: u32, sound_active: bool) {
        self.enqueue_step(SetlistKind::from_sound(sound_active), dir, now_ms);
    }

    /// Queue a step, or start it immediately if idle. Transitions play one at a time so an
    /// interrupted fade never pops; the queue is capped at the setlist length so mashing a
    /// button can't back up more than one full loop.
    fn enqueue_step(&mut self, kind: SetlistKind, dir: Step, now_ms: u32) {
        self.last_cycle_ms = now_ms;
        // A single-pattern setlist has nowhere to step; skip the pointless self-transition.
        let len = self.list(kind).len();
        if len < 2 {
            return;
        }
        if self.pending.len() < len {
            self.pending.push_back(dir);
        }
        self.pump_queue(kind, now_ms);
    }

    /// If idle and a step is queued, start the next transition from the current pattern.
    fn pump_queue(&mut self, kind: SetlistKind, now_ms: u32) {
        if self.transition.is_none() {
            if let Some(dir) = self.pending.pop_front() {
                self.begin_step(kind, dir, now_ms);
            }
        }
    }

    /// Advance/retreat the active setlist and start a transition from the old pattern.
    fn begin_step(&mut self, kind: SetlistKind, dir: Step, now_ms: u32) {
        let list = self.list(kind);
        let from_idx = list.idx;
        list.step(dir);
        let style = self.pick_style();
        self.begin_transition(kind, from_idx, now_ms, STEP_TRANSITION_MS, style);
    }

    pub fn toggle_hold(&mut self, now_ms: u32) {
        self.held = !self.held;
        // Restart the cycle countdown on release so unholding doesn't instantly auto-advance.
        if !self.held {
            self.last_cycle_ms = now_ms;
        }
    }

    /// Step global brightness one notch along BRIGHTNESS_LEVELS. `delta` is +1 (brighter)
    /// or -1 (dimmer); the index saturates at the ends (no wraparound).
    pub fn adjust_brightness(&mut self, delta: i32) {
        let last = (BRIGHTNESS_LEVELS.len() - 1) as i32;
        self.brightness_index = (self.brightness_index as i32 + delta).clamp(0, last) as usize;
    }

    /// Current global brightness multiplier (0.0-1.0), applied over the whole frame.
    pub fn brightness(&self) -> f32 {
        BRIGHTNESS_LEVELS[self.brightness_index]
    }

    /// Returns whether sound-reactive setlist should be active.
    /// `activity` comes from AudioReceiver's slow arm/release accumulator over the ear's
    /// level bytes, so Auto mode shifts deliberately, not per-beat. On/Off ignore it.
    pub fn sound_active(&self, activity: bool) -> bool {
        match self.sound_mode {
            SoundMode::Off  => false,
            SoundMode::On   => true,
            SoundMode::Auto => activity,
        }
    }
}
