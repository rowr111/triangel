use std::collections::VecDeque;

use crate::led::map::{Led, LED_COUNT};
use crate::patterns::transition::{self, TransitionStyle};
use crate::patterns::{Frame, Pattern};
use crate::patterns::ambient::{rainbow::RainbowX, ripple::ApexRipple, scan::HorizontalScan, shimmer::CenterShimmer};
use crate::patterns::reactive::audio_fill::AudioFill;

const CYCLE_MS: u32 = 3 * 60 * 1_000; // 3 minutes

// Transition durations: leisurely for manual/auto pattern steps, snappy for the
// ambient<->reactive sound flip so it tracks the music rather than lagging it.
const STEP_TRANSITION_MS:  u32 = 3000;
const SOUND_TRANSITION_MS: u32 = 200;

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
        Box::new(CenterShimmer  { speed: 60.0,  wavelength: 120.0 }),
        Box::new(RainbowX       { speed: 60.0 }),
        Box::new(ApexRipple     { speed: 100.0, wavelength: 80.0 }),
        Box::new(HorizontalScan { period_ms: 2_000, bandwidth: 30.0 }),
    ]
}

fn reactive_patterns() -> Vec<Box<dyn Pattern>> {
    vec![
        Box::new(AudioFill),
    ]
}

// --- A single ordered list of patterns with its own cursor ---

struct Setlist {
    patterns: Vec<Box<dyn Pattern>>,
    idx:      usize,
}

impl Setlist {
    fn next(&mut self) {
        self.idx = (self.idx + 1) % self.patterns.len();
    }

    fn prev(&mut self) {
        let len = self.patterns.len();
        self.idx = (self.idx + len - 1) % len;
    }
}

// --- Transitions ---

#[derive(Clone, Copy)]
enum SetlistKind {
    Ambient,
    Reactive,
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
enum Step {
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

// --- Setlist manager ---

pub struct SetlistManager {
    ambient:           Setlist,
    reactive:          Setlist,
    last_cycle_ms:     u32,
    held:              bool,
    transition:        Option<Transition>,
    pending:           VecDeque<Step>,
    last_sound_active: bool,
    next_style:        usize,
    pub brightness:    f32,
    pub sound_mode:    SoundMode,
}

impl SetlistManager {
    pub fn new(now_ms: u32) -> Self {
        SetlistManager {
            ambient:           Setlist { patterns: ambient_patterns(),  idx: 0 },
            reactive:          Setlist { patterns: reactive_patterns(), idx: 0 },
            last_cycle_ms:     now_ms,
            held:              false,
            transition:        None,
            pending:           VecDeque::new(),
            last_sound_active: false,
            next_style:        0,
            brightness:        1.0,
            sound_mode:        SoundMode::Off,
        }
    }

    fn setlist(&self, kind: SetlistKind) -> &Setlist {
        match kind { SetlistKind::Ambient => &self.ambient, SetlistKind::Reactive => &self.reactive }
    }

    fn setlist_mut(&mut self, kind: SetlistKind) -> &mut Setlist {
        match kind { SetlistKind::Ambient => &mut self.ambient, SetlistKind::Reactive => &mut self.reactive }
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

    fn render_into(&mut self, kind: SetlistKind, idx: usize, leds: &[Led], t_ms: u32, sound_level: f32, out: &mut Frame) {
        self.setlist_mut(kind).patterns[idx].render(leds, t_ms, sound_level, out);
    }

    /// Render the current frame, compositing an in-flight transition over it. Detects the
    /// ambient<->reactive flip and starts a fast crossfade for it.
    pub fn render(&mut self, leds: &[Led], t_ms: u32, sound_level: f32, sound_active: bool, out: &mut Frame) {
        // Sound flip is a hard setlist change: it jumps the queue (drops pending steps that
        // belong to the old setlist) and crossfades immediately.
        if sound_active != self.last_sound_active {
            let from_kind = SetlistKind::from_sound(self.last_sound_active);
            let from_idx  = self.setlist(from_kind).idx;
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
        let to_idx  = self.setlist(to_kind).idx;
        self.render_into(to_kind, to_idx, leds, t_ms, sound_level, out);

        // Composite the outgoing pattern over it while a transition is running.
        if let Some(tr) = self.transition {
            let progress = t_ms.wrapping_sub(tr.start_ms) as f32 / tr.duration_ms as f32;
            let mut from_buf: Frame = [[0u8; 3]; LED_COUNT];
            self.render_into(tr.from_kind, tr.from_idx, leds, t_ms, sound_level, &mut from_buf);
            transition::blend(tr.style, leds, progress, &from_buf, out);
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

    /// Advance one pattern. `now_ms` restarts the cycle countdown so a manual
    /// step doesn't immediately auto-advance.
    pub fn step_next(&mut self, now_ms: u32, sound_active: bool) {
        self.enqueue_step(SetlistKind::from_sound(sound_active), Step::Next, now_ms);
    }

    /// Step back one pattern; `now_ms` restarts the cycle countdown (see step_next).
    pub fn step_prev(&mut self, now_ms: u32, sound_active: bool) {
        self.enqueue_step(SetlistKind::from_sound(sound_active), Step::Prev, now_ms);
    }

    /// Queue a step, or start it immediately if idle. Transitions play one at a time so an
    /// interrupted fade never pops; the queue is capped at the setlist length so mashing a
    /// button can't back up more than one full loop.
    fn enqueue_step(&mut self, kind: SetlistKind, dir: Step, now_ms: u32) {
        self.last_cycle_ms = now_ms;
        if self.pending.len() < self.setlist(kind).patterns.len() {
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
        let from_idx = self.setlist(kind).idx;
        match dir {
            Step::Next => self.setlist_mut(kind).next(),
            Step::Prev => self.setlist_mut(kind).prev(),
        }
        let style = self.pick_style();
        self.begin_transition(kind, from_idx, now_ms, STEP_TRANSITION_MS, style);
    }

    pub fn toggle_hold(&mut self) {
        self.held = !self.held;
    }

    /// `delta` is positive (brighter) or negative (dimmer). Clamped to [0.05, 1.0].
    pub fn adjust_brightness(&mut self, delta: f32) {
        self.brightness = (self.brightness + delta).clamp(0.05, 1.0);
    }

    /// Returns whether sound-reactive setlist should be active.
    /// `activity` is the flag from the ear chip - sustained absolute loudness above its
    /// calibrated threshold. Used only in Auto mode; On/Off ignore it.
    pub fn sound_active(&self, activity: bool) -> bool {
        match self.sound_mode {
            SoundMode::Off  => false,
            SoundMode::On   => true,
            SoundMode::Auto => activity,
        }
    }
}
