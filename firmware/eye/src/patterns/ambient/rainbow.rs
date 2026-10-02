use crate::patterns::{Frame, Pattern, hsv, wrap360};
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::Led;
use core::f32::consts::TAU;

// Rainbow - hue follows the angle around the centroid, so the whole wheel rotates.

// Effect strengths, 0.0 (off) to 1.0 (full).
const TWINKLE:     f32 = 0.7; // swirl of fading around each triangle tile
const BREATHE:     f32 = 0.5; // rotation eases like a tide
const IRIDESCENCE: f32 = 0.2; // drifting hue ripple - oil-on-water shimmer

const SPIN_PERIOD_MS: u32 = 20_000; // one full rotation of the wheel

// Breathe: depth of the speed wobble as a fraction of a hue cycle, over this period.
const BREATHE_DEPTH:     f32 = 0.15;
const BREATHE_PERIOD_MS: u32 = 18_000;

const TWINKLE_RATE: f32 = 0.0026; // rad/ms; ~2.4 s per cycle

// Iridescence: a concentric hue ripple drifting outward. Ring spacing, max hue swing,
// drift time.
const IRID_SPAN_MM:   f32 = 70.0;
const IRID_DEG:       f32 = 55.0;
const IRID_PERIOD_MS: u32 = 9_000;

pub struct Rainbow;

impl Pattern for Rainbow {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        // Each time term is folded to its own period before the f32 cast: raw t_ms loses
        // sub-frame precision after hours of uptime.
        let breathe_ph = (t_ms % BREATHE_PERIOD_MS) as f32 / BREATHE_PERIOD_MS as f32 * TAU;
        let breathe = BREATHE * BREATHE_DEPTH * breathe_ph.sin();
        let spin = (t_ms % SPIN_PERIOD_MS) as f32 / SPIN_PERIOD_MS as f32 + breathe;
        let twinkle_period = (TAU / TWINKLE_RATE) as u32;
        let twinkle_t = (t_ms % twinkle_period.max(1)) as f32;
        let irid_ph = (t_ms % IRID_PERIOD_MS) as f32 / IRID_PERIOD_MS as f32 * TAU;

        for (i, led) in leds.iter().enumerate() {
            let ripple = (DIST_C[i] / IRID_SPAN_MM * TAU - irid_ph).sin();
            let hue = (THETA_C[i] / TAU + spin) * 360.0 + IRIDESCENCE * IRID_DEG * ripple;

            // A fade that circulates each tile: the phase ramps with local_idx.
            let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
            let dip = 0.5 - 0.5 * (twinkle_t * TWINKLE_RATE + hash as f32).sin();

            out[i] = hsv(wrap360(hue), 1.0, 1.0 - TWINKLE * dip);
        }
    }
}
