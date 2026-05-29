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

// --- Sound mode ---

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SoundMode {
    Off,
    Auto,
    On,
}

// --- Setlist manager ---

pub struct SetlistManager {
    ambient:        Vec<Box<dyn Pattern>>,
    reactive:       Vec<Box<dyn Pattern>>,
    pattern_idx:    usize,
    last_cycle_ms:  u32,
    held:           bool,
    pub brightness: f32,
    pub sound_mode: SoundMode,
}

impl SetlistManager {
    pub fn new(now_ms: u32) -> Self {
        SetlistManager {
            ambient:       ambient_patterns(),
            reactive:      reactive_patterns(),
            pattern_idx:   0,
            last_cycle_ms: now_ms,
            held:          false,
            brightness:    1.0,
            sound_mode:    SoundMode::On, // TODO: revert to Off before production
        }
    }

    fn list_len(&self, sound_active: bool) -> usize {
        if sound_active { self.reactive.len() } else { self.ambient.len() }
    }

    pub fn current_pattern(&mut self, sound_active: bool) -> &mut dyn Pattern {
        let list = if sound_active { &mut self.reactive } else { &mut self.ambient };
        let idx = self.pattern_idx.min(list.len() - 1);
        list[idx].as_mut()
    }

    /// Call once per frame. Advances pattern index when the cycle timer expires.
    pub fn tick(&mut self, t_ms: u32, sound_active: bool) {
        if self.held {
            return;
        }
        if t_ms.wrapping_sub(self.last_cycle_ms) >= CYCLE_MS {
            self.pattern_idx = (self.pattern_idx + 1) % self.list_len(sound_active);
            self.last_cycle_ms = t_ms;
        }
    }

    pub fn step_next(&mut self, sound_active: bool) {
        self.pattern_idx = (self.pattern_idx + 1) % self.list_len(sound_active);
        self.last_cycle_ms = 0;
    }

    pub fn step_prev(&mut self, sound_active: bool) {
        let len = self.list_len(sound_active);
        self.pattern_idx = (self.pattern_idx + len - 1) % len;
        self.last_cycle_ms = 0;
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
