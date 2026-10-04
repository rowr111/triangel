pub mod ambient;
pub mod glints;
pub mod reactive;
pub mod ripples;
pub mod transition;

use core::f32::consts::TAU;

use crate::audio::Audio;
use crate::led::map::Led;

pub type Frame = [[u8; 3]; crate::led::map::LED_COUNT];

/// A pattern in either setlist. `out[i]` is the color of `LED_MAP[i]`. Ambient patterns
/// ignore `audio`; sound-reactive ones have to look interesting at every level, silence
/// included.
pub trait Pattern: Send {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame);

    /// Called each time the pattern comes on screen, before its first frame there.
    fn on_enter(&mut self, _t_ms: u32) {}
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

/// Smooth 0-1 ramp; `t` is clamped to 0-1 first.
pub fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

pub fn mix_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// Color at `t` along a ramp of (position, color) stops, positions rising from 0 to 1.
pub fn ramp(stops: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    for pair in stops.windows(2) {
        let (t0, c0) = pair[0];
        let (t1, c1) = pair[1];
        if t <= t1 {
            return mix_rgb(c0, c1, (t - t0) / (t1 - t0));
        }
    }
    stops[stops.len() - 1].1
}

/// Where `t_ms` sits in a repeating period, 0-1. Folding first keeps f32 precise after
/// hours of uptime.
pub fn cycle(t_ms: u32, period_ms: u32) -> f32 {
    (t_ms % period_ms) as f32 / period_ms as f32
}

/// `t_ms` folded to one period of a `rate` in rad/ms, for `sin(t * rate + ...)`. The
/// period is rounded to whole ms, which leaves a seam far under a degree.
pub fn fold_ms(t_ms: u32, rate: f32) -> f32 {
    let period = (TAU / rate) as u32;
    (t_ms % period.max(1)) as f32
}

/// Bit-mix hash of two u32s into a scrambled u32.
pub fn hash2(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 13;
    h
}

/// Knuth's multiplicative hash: spreads consecutive inputs far apart.
pub fn knuth_hash(x: u32) -> u32 {
    x.wrapping_mul(2654435761)
}

/// Distinct values `tile_hash` can take.
pub const TILE_HASH_STEPS: u32 = 97;

/// Cheap per-LED value from its tile and position on the tile, 0..TILE_HASH_STEPS.
pub fn tile_hash(led: &Led) -> u32 {
    (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % TILE_HASH_STEPS
}

/// Index of the first slot `is_free` accepts, or else the one that started longest ago.
pub fn free_or_oldest<T>(
    slots: &[T],
    now_ms: u32,
    is_free: impl Fn(&T) -> bool,
    start_ms: impl Fn(&T) -> u32,
) -> usize {
    let age = |s: &T| now_ms.wrapping_sub(start_ms(s));
    slots.iter().position(&is_free).unwrap_or_else(|| {
        let mut oldest = 0;
        for (i, s) in slots.iter().enumerate() {
            if age(s) > age(&slots[oldest]) {
                oldest = i;
            }
        }
        oldest
    })
}

/// One flash on a line or tile: lit at `start_ms`, full for `hold_ms`, then fading out
/// over `fade_ms`.
#[derive(Clone, Copy)]
pub struct Shot {
    pub start_ms: u32,
    pub strength: f32,
    pub hue:      f32,
    pub sat:      f32,
    pub hold_ms:  f32,
    pub fade_ms:  f32,
}

impl Shot {
    pub const NONE: Shot =
        Shot { start_ms: 0, strength: 0.0, hue: 0.0, sat: 0.0, hold_ms: 0.0, fade_ms: 0.0 };

    /// Age in ms and brightness 0-1, full through the hold and then falling to 0. None while
    /// unlit, not started yet, or finished.
    pub fn level(&self, t_ms: u32) -> Option<(f32, f32)> {
        if self.strength <= 0.0 {
            return None;
        }
        let age = t_ms.wrapping_sub(self.start_ms) as i32 as f32;
        if age < 0.0 || age >= self.hold_ms + self.fade_ms {
            return None;
        }
        let fade = if age < self.hold_ms { 1.0 } else { 1.0 - (age - self.hold_ms) / self.fade_ms };
        Some((age, fade))
    }

    pub fn end_ms(&self) -> u32 {
        self.start_ms.wrapping_add((self.hold_ms + self.fade_ms) as u32)
    }
}

/// xorshift32. Each pattern seeds its own, so its sequence is fixed.
#[derive(Clone, Copy)]
pub struct Rng(u32);

impl Rng {
    pub const fn new(seed: u32) -> Self {
        Rng(seed)
    }

    pub fn next_u32(&mut self) -> u32 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.0 = x;
        x
    }

    /// Uniform in [0, 1).
    pub fn f32(&mut self) -> f32 {
        (self.next_u32() >> 8) as f32 / 16_777_216.0
    }

    /// Uniform whole number in [min, max).
    pub fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        min + self.next_u32() % (max - min)
    }

    /// One entry of `items`, uniformly.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[(self.f32() * items.len() as f32) as usize % items.len()]
    }
}
