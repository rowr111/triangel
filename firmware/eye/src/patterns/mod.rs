pub mod rainbow;
pub mod ripple;
pub mod scan;
pub mod shimmer;

use crate::led::map::Led;

pub type Frame = [[u8; 3]; crate::led::map::LED_COUNT];

pub trait Pattern: Send {
    /// Render one frame into `out`.
    /// `leds`        — world-position metadata for each LED, indexed by chain position
    /// `t_ms`        — monotonic time in milliseconds
    /// `sound_level` — smoothed normalised sound level 0.0–1.0 (ignored by ambient patterns)
    fn render(&mut self, leds: &[Led], t_ms: u32, sound_level: f32, out: &mut Frame);
}

// ─── Envelope ─────────────────────────────────────────────────────────────────

/// Attack/decay envelope for sound-reactive patterns.
/// Hold one as a field on your pattern struct and call `update()` each frame.
pub struct Envelope {
    pub attack: f32,
    pub decay:  f32,
    value:      f32,
}

impl Envelope {
    pub fn new(attack: f32, decay: f32) -> Self {
        Envelope { attack, decay, value: 0.0 }
    }

    /// Feed a new input sample (0.0–1.0), returns the smoothed value.
    pub fn update(&mut self, input: f32) -> f32 {
        if input > self.value {
            self.value += self.attack * (input - self.value);
        } else {
            self.value = (self.value - self.decay).max(input).max(0.0);
        }
        self.value
    }
}

// ─── Shared math utilities ────────────────────────────────────────────────────

/// HSV → RGB. h: 0–360, s/v: 0–1. Returns [r, g, b] each 0–255.
pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let f = |n: f32| -> f32 {
        let k = (n + h / 60.0) % 6.0;
        v - v * s * k.min(4.0 - k).min(1.0_f32).max(0.0)
    };
    [(f(5.0) * 255.0) as u8, (f(3.0) * 255.0) as u8, (f(1.0) * 255.0) as u8]
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

pub fn clamp(x: f32, lo: f32, hi: f32) -> f32 { x.max(lo).min(hi) }
