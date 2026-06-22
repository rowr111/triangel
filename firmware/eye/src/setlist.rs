use crate::patterns::Pattern;
use crate::patterns::{audio_fill::AudioFill, rainbow::RainbowX, ripple::ApexRipple, scan::HorizontalScan, shimmer::CenterShimmer};

const CYCLE_MS: u32 = 3 * 60 * 1_000; // 3 minutes

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
    fn current(&mut self) -> &mut dyn Pattern {
        self.patterns[self.idx].as_mut()
    }

    fn next(&mut self) {
        self.idx = (self.idx + 1) % self.patterns.len();
    }

    fn prev(&mut self) {
        let len = self.patterns.len();
        self.idx = (self.idx + len - 1) % len;
    }
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
    ambient:        Setlist,
    reactive:       Setlist,
    last_cycle_ms:  u32,
    held:           bool,
    pub brightness: f32,
    pub sound_mode: SoundMode,
}

impl SetlistManager {
    pub fn new(now_ms: u32) -> Self {
        SetlistManager {
            ambient:       Setlist { patterns: ambient_patterns(),  idx: 0 },
            reactive:      Setlist { patterns: reactive_patterns(), idx: 0 },
            last_cycle_ms: now_ms,
            held:          false,
            brightness:    1.0,
            sound_mode:    SoundMode::Off,
        }
    }

    /// The live setlist for the current sound mode. Each list keeps its own cursor,
    /// so toggling sound on/off preserves each one's position independently.
    fn active(&mut self, sound_active: bool) -> &mut Setlist {
        if sound_active { &mut self.reactive } else { &mut self.ambient }
    }

    pub fn current_pattern(&mut self, sound_active: bool) -> &mut dyn Pattern {
        self.active(sound_active).current()
    }

    /// Call once per frame. Advances pattern index when the cycle timer expires.
    pub fn tick(&mut self, t_ms: u32, sound_active: bool) {
        if self.held {
            return;
        }
        if t_ms.wrapping_sub(self.last_cycle_ms) >= CYCLE_MS {
            self.active(sound_active).next();
            self.last_cycle_ms = t_ms;
        }
    }

    /// Advance one pattern. `now_ms` restarts the cycle countdown so a manual
    /// step doesn't immediately auto-advance.
    pub fn step_next(&mut self, now_ms: u32, sound_active: bool) {
        self.active(sound_active).next();
        self.last_cycle_ms = now_ms;
    }

    /// Step back one pattern; `now_ms` restarts the cycle countdown (see step_next).
    pub fn step_prev(&mut self, now_ms: u32, sound_active: bool) {
        self.active(sound_active).prev();
        self.last_cycle_ms = now_ms;
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
