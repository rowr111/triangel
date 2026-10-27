pub mod ambient;
pub mod glints;
pub mod reactive;
pub mod ripples;
pub mod transition;

use core::f32::consts::TAU;
use core::sync::atomic::{AtomicU32, Ordering};

use crate::audio::Audio;
use crate::led::map::Led;

pub type Frame = [[u8; 3]; crate::led::map::LED_COUNT];

/// A pattern in either setlist. `out[i]` is the color of `LED_MAP[i]`. Ambient patterns
/// ignore `audio`.
pub trait Pattern: Send {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame);

    /// Called each time the pattern comes on screen, before its first frame there.
    fn on_enter(&mut self, _t_ms: u32) {}
}

/// HSV to RGB. h is 0-360, s and v are 0-1.
pub fn hsv(h: f32, s: f32, v: f32) -> [u8; 3] {
    let h60 = h / 60.0;
    let f = |n: f32| -> f32 {
        // n + h60 is in [1, 11), so one subtraction wraps it.
        let mut k = n + h60;
        if k >= 6.0 {
            k -= 6.0;
        }
        v - v * s * k.min(4.0 - k).clamp(0.0, 1.0_f32)
    };
    [
        (f(5.0) * 255.0) as u8,
        (f(3.0) * 255.0) as u8,
        (f(1.0) * 255.0) as u8,
    ]
}

/// HSV to RGB in whole numbers. `h6` is sixths of a turn x4096, `s` is x4096, `v` is 0-255.
pub fn hsv_int(h6: i32, s: i32, v: i32) -> [u8; 3] {
    let channel = |offset: i32| -> u8 {
        let mut pos = offset * 4096 + h6;
        if pos >= 6 * 4096 {
            pos -= 6 * 4096;
        }
        let cut = pos.min(4 * 4096 - pos).clamp(0, 4096);
        ((v * (4096 - ((s * cut) >> 12))) >> 12) as u8
    };
    [channel(5), channel(3), channel(1)]
}

pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// The sin and cos of one angle, so the sine of a sum of angles needs no `sin` call.
#[derive(Clone, Copy)]
pub struct Phasor {
    pub sin: f32,
    pub cos: f32,
}

impl Phasor {
    pub fn new(angle: f32) -> Phasor {
        let (sin, cos) = angle.sin_cos();
        Phasor { sin, cos }
    }

    /// sin(self + other).
    pub fn sin_plus(self, other: Phasor) -> f32 {
        self.sin * other.cos + self.cos * other.sin
    }

    /// sin(self - other).
    pub fn sin_minus(self, other: Phasor) -> f32 {
        self.sin * other.cos - self.cos * other.sin
    }
}

/// Number of phase offsets in `phase_phasors`, 0.01 rad apart, covering 0..TAU.
pub const PHASE_STEPS: usize = 628;

/// Each phase offset as a `Phasor`. With the frame's angle, `sin_plus` gives
/// sin(offset + angle) without a sin call per LED.
pub fn phase_phasors() -> &'static [Phasor; PHASE_STEPS] {
    static PHASORS: std::sync::OnceLock<[Phasor; PHASE_STEPS]> = std::sync::OnceLock::new();
    PHASORS.get_or_init(|| core::array::from_fn(|k| Phasor::new(k as f32 / 100.0)))
}

/// Wraps a hue into [0, 360). The input must be within one turn of that range.
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
    [
        lerp(a[0], b[0], t),
        lerp(a[1], b[1], t),
        lerp(a[2], b[2], t),
    ]
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

/// Where `t_ms` sits in a repeating period, 0-1.
pub fn cycle(t_ms: u32, period_ms: u32) -> f32 {
    (t_ms % period_ms) as f32 / period_ms as f32
}

/// Where `t_ms` sits in a repeating period, 0-1 x65536.
pub fn cycle_q16(t_ms: u32, period_ms: u32) -> u32 {
    ((t_ms % period_ms) as u64 * 65536 / period_ms as u64) as u32
}

/// Which lifetime a repeating slot is in and how far through, 0-1. Its length is hashed.
pub fn slot_life(slot: u32, salt: u32, t_ms: u32, min_ms: u32, max_ms: u32) -> (u32, f32) {
    let slot_seed = hash2(slot, salt);
    let life = min_ms + slot_seed % (max_ms - min_ms);
    let local = t_ms.wrapping_add(slot_seed % life);
    (local / life, cycle(local, life))
}

/// `t_ms` wrapped to one period of a `rate` in rad/ms, for `sin(t * rate + ...)`.
pub fn fold_ms(t_ms: u32, rate: f32) -> f32 {
    let period = (TAU / rate) as u32;
    (t_ms % period.max(1)) as f32
}

/// Milliseconds since `last_ms`, at most `max_ms`; also sets `last_ms` to `t_ms`.
pub fn time_step_ms(last_ms: &mut u32, t_ms: u32, max_ms: u32) -> u32 {
    let step = t_ms.wrapping_sub(*last_ms).min(max_ms);
    *last_ms = t_ms;
    step
}

/// Hash of two u32s.
pub fn hash2(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 13;
    h
}

/// A value in [0, 1) from a hash of `a` and `b`, so each `b` gives a different value.
pub fn hash01(a: u32, b: u32) -> f32 {
    (hash2(a, b) >> 8) as f32 / 16_777_216.0
}

/// Knuth's multiplicative hash: spreads consecutive inputs far apart.
pub fn knuth_hash(x: u32) -> u32 {
    x.wrapping_mul(2654435761)
}

/// Distinct values `tile_hash` can take.
pub const TILE_HASH_STEPS: u32 = 97;

/// Per-LED value from its tile and position on the tile, 0..TILE_HASH_STEPS.
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

/// One flash: full brightness for `hold_ms` from `start_ms`, then fading over `fade_ms`.
#[derive(Clone, Copy)]
pub struct Shot {
    pub start_ms: u32,
    pub strength: f32,
    pub hue: f32,
    pub sat: f32,
    pub hold_ms: f32,
    pub fade_ms: f32,
    /// How long it stays white, and how much brighter it is then.
    pub white_ms: f32,
    pub boost: f32,
}

/// A shot's color at one moment.
#[derive(Clone, Copy, Default)]
pub struct ShotColor {
    pub hue: f32,
    pub sat: f32,
    pub brightness: f32,
}

impl Shot {
    pub const NONE: Shot = Shot {
        start_ms: 0,
        strength: 0.0,
        hue: 0.0,
        sat: 0.0,
        hold_ms: 0.0,
        fade_ms: 0.0,
        white_ms: 0.0,
        boost: 0.0,
    };

    /// Its color at `t_ms`, or None if unlit, not started yet, or finished.
    fn color_at(&self, t_ms: u32) -> Option<ShotColor> {
        if self.strength <= 0.0 {
            return None;
        }
        let age = t_ms.wrapping_sub(self.start_ms) as i32 as f32;
        if age < 0.0 || age >= self.hold_ms + self.fade_ms {
            return None;
        }
        let fade = if age < self.hold_ms {
            1.0
        } else {
            1.0 - (age - self.hold_ms) / self.fade_ms
        };
        let white = (1.0 - age / self.white_ms).clamp(0.0, 1.0);
        Some(ShotColor {
            hue: self.hue,
            sat: self.sat * (1.0 - white),
            brightness: (self.strength * fade * fade * (1.0 + self.boost * white)).min(1.0),
        })
    }

    fn end_ms(&self) -> u32 {
        self.start_ms
            .wrapping_add((self.hold_ms + self.fade_ms) as u32)
    }
}

/// The two latest shots on one part of the Triangel; the brighter one shows.
#[derive(Clone, Copy)]
pub struct ShotPair([Shot; 2]);

impl ShotPair {
    pub const NONE: ShotPair = ShotPair([Shot::NONE; 2]);

    /// Replaces the shot that ends sooner, but a waiting one before a showing one, so the
    /// light does not go dark.
    pub fn add(&mut self, new: Shot, t_ms: u32) {
        let new_left = new.end_ms().wrapping_sub(t_ms) as i32;
        // The lower one is replaced.
        let rank = |s: &Shot| {
            let left = s.end_ms().wrapping_sub(t_ms) as i32;
            let waiting = s.start_ms.wrapping_sub(t_ms) as i32 > 0;
            if s.strength <= 0.0 {
                i32::MIN
            } else if waiting && left <= new_left {
                i32::MIN + 1
            } else {
                left
            }
        };
        let slot = if rank(&self.0[0]) <= rank(&self.0[1]) {
            0
        } else {
            1
        };
        self.0[slot] = new;
    }

    /// The color of the brighter shot at `t_ms`, if either is lit.
    pub fn brightest(&mut self, t_ms: u32) -> Option<ShotColor> {
        let mut best: Option<ShotColor> = None;
        for shot in &mut self.0 {
            // Forget a finished flash; its end time goes stale.
            if shot.strength > 0.0 && t_ms.wrapping_sub(shot.end_ms()) as i32 >= 0 {
                shot.strength = 0.0;
            }
            if let Some(color) = shot.color_at(t_ms) {
                if color.brightness > best.map_or(0.0, |b| b.brightness) {
                    best = Some(color);
                }
            }
        }
        best
    }
}

// Mixed into every seed.
static BOOT_SEED: AtomicU32 = AtomicU32::new(0);

/// Call before any pattern is built.
pub fn set_boot_seed(seed: u32) {
    BOOT_SEED.store(seed, Ordering::Relaxed);
}

// The function random_u32 calls for its numbers. Empty until main sets it.
static RANDOM_SOURCE: std::sync::OnceLock<fn() -> Option<u32>> = std::sync::OnceLock::new();
static FIXED_SEQUENCE: AtomicU32 = AtomicU32::new(0);

/// Call once at boot with the hardware random number source.
pub fn set_random_source(source: fn() -> Option<u32>) {
    RANDOM_SOURCE.set(source).ok();
}

/// A new random number. A fixed sequence if no source is set.
pub fn random_u32() -> u32 {
    RANDOM_SOURCE
        .get()
        .and_then(|source| source())
        .unwrap_or_else(|| hash2(FIXED_SEQUENCE.fetch_add(1, Ordering::Relaxed), 0))
}

/// xorshift32. Each pattern seeds its own.
pub struct Rng(u32);

impl Rng {
    /// A zero seed would produce only zeros, so it is swapped for a fixed number.
    pub fn new(seed: u32) -> Self {
        let seed = seed ^ BOOT_SEED.load(Ordering::Relaxed);
        Rng(if seed == 0 { 0x9E37_79B9 } else { seed })
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

    /// Uniform in [min, max).
    pub fn range_f32(&mut self, min: f32, max: f32) -> f32 {
        min + self.f32() * (max - min)
    }

    /// Uniform in [-x, x).
    pub fn plus_minus(&mut self, x: f32) -> f32 {
        (self.f32() - 0.5) * 2.0 * x
    }

    /// Uniform whole number in [min, max), or `min` if the range is empty.
    pub fn range_u32(&mut self, min: u32, max: u32) -> u32 {
        if max <= min {
            return min;
        }
        min + self.next_u32() % (max - min)
    }

    /// Uniform whole number in [0, n).
    pub fn below(&mut self, n: usize) -> usize {
        (self.f32() * n as f32) as usize % n
    }

    /// One entry of `items`, uniformly.
    pub fn pick<T: Copy>(&mut self, items: &[T]) -> T {
        items[self.below(items.len())]
    }
}
