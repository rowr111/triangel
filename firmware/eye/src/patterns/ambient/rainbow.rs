use crate::patterns::glints::{GlintStyle, Glints};
use crate::patterns::{Frame, Pattern, cycle, fold_ms, hsv, tile_hash, wrap360};
use crate::led::geom::{DIST_C, THETA_C};
use crate::audio::Audio;
use crate::led::map::LED_MAP;
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
}

impl Rainbow {
    pub fn new() -> Self {
        Rainbow { glints: Glints::new(0x3C6E_F372, GLINTS), last_ms: 0 }
    }
}

impl Pattern for Rainbow {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_ms = t_ms.wrapping_sub(self.last_ms).min(100);
        self.last_ms = t_ms;

        let breathe = BREATHE * BREATHE_DEPTH * (cycle(t_ms, BREATHE_PERIOD_MS) * TAU).sin();
        let spin = cycle(t_ms, SPIN_PERIOD_MS) + breathe;
        let twinkle_t = fold_ms(t_ms, TWINKLE_RATE);
        let irid_ph = cycle(t_ms, IRID_PERIOD_MS) * TAU;

        for (i, led) in LED_MAP.iter().enumerate() {
            let ripple = (DIST_C[i] / IRID_SPAN_MM * TAU - irid_ph).sin();
            let hue = (THETA_C[i] / TAU + spin) * 360.0 + IRIDESCENCE * IRID_DEG * ripple;

            let dip = 0.5 - 0.5 * (twinkle_t * TWINKLE_RATE + tile_hash(led) as f32).sin();

            out[i] = hsv(wrap360(hue), 1.0, 1.0 - TWINKLE * dip);
        }

        self.glints.update(t_ms, dt_ms, out);
    }
}
