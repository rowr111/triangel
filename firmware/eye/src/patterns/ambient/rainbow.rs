use crate::patterns::glints::{GlintStyle, Glints};
use crate::patterns::{Frame, Pattern, cycle, fold_ms, hsv, sin_sum, tile_hash, wrap360, TILE_HASH_STEPS};
use crate::led::geom::{DIST_C, THETA_C};
use crate::audio::Audio;
use crate::led::map::{LED_COUNT, LED_MAP};
use core::f32::consts::TAU;

// Rainbow: hue follows the angle around the center, and the wheel rotates.

// Effect strengths, 0.0 (off) to 1.0 (full).
const TWINKLE:     f32 = 0.7; // a fade that circles each tile
const BREATHE:     f32 = 0.5; // the rotation speeds up and slows down
const IRIDESCENCE: f32 = 0.2; // a hue ripple moving outward

const SPIN_PERIOD_MS: u32 = 20_000; // one full rotation

const BREATHE_DEPTH:     f32 = 0.15; // as a fraction of a hue cycle
const BREATHE_PERIOD_MS: u32 = 18_000;

const TWINKLE_RATE: f32 = 0.0026; // rad/ms, about 2.4 s per cycle

const IRID_SPAN_MM:   f32 = 70.0; // ring spacing
const IRID_DEG:       f32 = 55.0; // most the hue shifts
const IRID_PERIOD_MS: u32 = 9_000;

const GLINTS: GlintStyle = GlintStyle {
    tries_per_sec: 10.0,
    min_ms:        250, // fade time
    max_ms:        450,
    white:         0.75, // how far toward white a glint starts
};

pub struct Rainbow {
    glints:  Glints,
    last_ms: u32,
    // Fixed per LED: sin and cos of its ripple angle, its place on the hue wheel (0-1)
    // and its tile hash.
    ripple_sin: [f32; LED_COUNT],
    ripple_cos: [f32; LED_COUNT],
    hue_turn:   [f32; LED_COUNT],
    tile:       [u8; LED_COUNT],
}

impl Rainbow {
    pub fn new() -> Self {
        let ripple_at = |i: usize| DIST_C[i] / IRID_SPAN_MM * TAU;
        Rainbow {
            glints:     Glints::new(0x3C6E_F372, GLINTS),
            last_ms:    0,
            ripple_sin: core::array::from_fn(|i| ripple_at(i).sin()),
            ripple_cos: core::array::from_fn(|i| ripple_at(i).cos()),
            hue_turn:   core::array::from_fn(|i| THETA_C[i] / TAU),
            tile:       core::array::from_fn(|i| tile_hash(&LED_MAP[i]) as u8),
        }
    }
}

impl Pattern for Rainbow {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_ms = t_ms.wrapping_sub(self.last_ms).min(100);
        self.last_ms = t_ms;

        let breathe = BREATHE * BREATHE_DEPTH * (cycle(t_ms, BREATHE_PERIOD_MS) * TAU).sin();
        let spin = cycle(t_ms, SPIN_PERIOD_MS) + breathe;
        let twinkle_ph = fold_ms(t_ms, TWINKLE_RATE) * TWINKLE_RATE;
        let (irid_sin, irid_cos) = (cycle(t_ms, IRID_PERIOD_MS) * TAU).sin_cos();
        // The twinkle wave for each value a tile hash can take.
        let twinkle: [f32; TILE_HASH_STEPS as usize] = core::array::from_fn(|k| (twinkle_ph + k as f32).sin());

        for (i, px) in out.iter_mut().enumerate() {
            // sin(ripple angle - ripple phase)
            let ripple = sin_sum(self.ripple_sin[i], self.ripple_cos[i], -irid_sin, irid_cos);
            let hue = (self.hue_turn[i] + spin) * 360.0 + IRIDESCENCE * IRID_DEG * ripple;

            let dip = 0.5 - 0.5 * twinkle[self.tile[i] as usize];

            *px = hsv(wrap360(hue), 1.0, 1.0 - TWINKLE * dip);
        }

        self.glints.update(t_ms, dt_ms, out);
    }
}
