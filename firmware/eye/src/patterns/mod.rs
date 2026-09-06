pub mod ambient;
pub mod reactive;
#[allow(dead_code)] // bench patterns, unused until one is added to a setlist by hand
pub mod test;
pub mod transition;

use crate::audio::Audio;
use crate::led::map::Led;

pub type Frame = [[u8; 3]; crate::led::map::LED_COUNT];

/// A pattern in the ambient setlist.
pub trait Pattern: Send {
    /// Render one frame into `out`.
    /// `leds` - world-position metadata for each LED, indexed by chain position
    /// `t_ms` - monotonic time in milliseconds
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame);
}

/// A pattern in the sound-reactive setlist. Has to look interesting at every level,
/// silence included.
pub trait ReactivePattern: Send {
    fn render(&mut self, leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame);
}

// --- Envelope ---

/// Attack/decay envelope for a reactive pattern that wants to rise and fall at its
/// own rate. Hold one as a field and call `update()` each frame.
#[allow(dead_code)]
pub struct Envelope {
    pub attack: f32,
    pub decay:  f32,
    value:      f32,
}

#[allow(dead_code)]
impl Envelope {
    pub fn new(attack: f32, decay: f32) -> Self {
        Envelope { attack, decay, value: 0.0 }
    }

    /// Feed a new input sample (0.0-1.0), returns the smoothed value.
    pub fn update(&mut self, input: f32) -> f32 {
        if input > self.value {
            self.value += self.attack * (input - self.value);
        } else {
            self.value = (self.value - self.decay).max(input).max(0.0);
        }
        self.value
    }
}

// --- Shared math utilities ---

/// HSV -> RGB. h: 0-360, s/v: 0-1. Returns [r, g, b] each 0-255.
pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h60 = h / 60.0;
    let f = |n: f32| -> f32 {
        // n + h60 is inside [1, 11), so wrapping is at most one subtraction.
        let mut k = n + h60;
        if k >= 6.0 {
            k -= 6.0;
        }
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0_f32)
    };
    [(f(5.0) * 255.0) as u8, (f(3.0) * 255.0) as u8, (f(1.0) * 255.0) as u8]
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 { a + (b - a) * t }

/// Steps the patterns' hashes quantize a 0..TAU phase offset into, 0.01 rad each.
pub const PHASE_STEPS: usize = 628;

/// sin and cos of every hash-quantized phase offset, for rotating a per-LED phasor by a
/// per-frame angle instead of calling sin per LED.
pub fn phase_phasors() -> &'static ([f32; PHASE_STEPS], [f32; PHASE_STEPS]) {
    static T: std::sync::OnceLock<([f32; PHASE_STEPS], [f32; PHASE_STEPS])> =
        std::sync::OnceLock::new();
    T.get_or_init(|| {
        let mut sn = [0.0; PHASE_STEPS];
        let mut cs = [0.0; PHASE_STEPS];
        for k in 0..PHASE_STEPS {
            let (s, c) = (k as f32 / 100.0).sin_cos();
            sn[k] = s;
            cs[k] = c;
        }
        (sn, cs)
    })
}

/// Wrap a hue into [0, 360). Exact for inputs within one turn of range.
pub fn wrap360(h: f32) -> f32 {
    if h >= 360.0 {
        h - 360.0
    } else if h < 0.0 {
        h + 360.0
    } else {
        h
    }
}
