use core::f32::consts::TAU;

use crate::audio::{Audio, MEL_BANDS};
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, ReactivePattern, hsv, lerp};

// Hue in fractions of the color wheel rather than degrees, so wrapping the sum is a
// floor rather than a modulo. Core to rim runs gold, red, magenta, cyan.
const HUE_BASE_TURN:   f32 = 0.125;
const HUE_SWEEP_TURNS: f32 = -0.61;
const BAND_TO_TURN:    f32 = HUE_SWEEP_TURNS / (MEL_BANDS - 1) as f32;
// How far the ripple bends the colors, and how long the whole wheel takes to come
// back around.
const HUE_WOBBLE_TURNS: f32 = 0.10;
const HUE_SPIN_MS:      u32 = 30_000;

// Floor under the loudness scale, so a quiet room still shows the band shape.
const QUIET_FLOOR: f32 = 0.20;

// Steepens the brightness ramp. The response stays linear, so contrast holds; the
// loudest bands reach full and clip there.
const GAIN: f32 = 1.7;

// A band's onset adds this much brightness and whitens it by this much.
const HIT_GAIN:  f32 = 0.6;
const HIT_WHITE: f32 = 0.65;

// Slow angular ripple, so the rings breathe instead of sitting still.
const LOBES:            f32 = 3.0;
const SWIRL_DEPTH:      f32 = 0.18;
const SWIRL_PERIOD_MS:  u32 = 12_000;

/// The 24 mel bands mapped onto distance from the fixture centroid: bass at the core,
/// treble at the rim. Bands are assigned by radius rank rather than by radius, so each
/// one drives the same number of LEDs - by radius the outermost bands would land on
/// six LEDs apiece and the treble would not read at all.
pub struct Spectrum {
    /// Fractional band index per LED, 0.0 to MEL_BANDS-1, for interpolating neighbors.
    band_pos: [f32; LED_COUNT],
    /// sin and cos of LOBES * theta, so the ripple rotates without a per-LED sin call.
    swirl_s: [f32; LED_COUNT],
    swirl_c: [f32; LED_COUNT],
}

impl Spectrum {
    pub fn new() -> Self {
        let mut order: [usize; LED_COUNT] = core::array::from_fn(|i| i);
        order.sort_unstable_by(|&a, &b| {
            DIST_C[a].partial_cmp(&DIST_C[b]).unwrap_or(core::cmp::Ordering::Equal)
        });
        let mut band_pos = [0.0f32; LED_COUNT];
        let span = (MEL_BANDS - 1) as f32 / LED_COUNT as f32;
        for (rank, &i) in order.iter().enumerate() {
            band_pos[i] = rank as f32 * span;
        }
        Spectrum {
            band_pos,
            swirl_s: core::array::from_fn(|i| (LOBES * THETA_C[i]).sin()),
            swirl_c: core::array::from_fn(|i| (LOBES * THETA_C[i]).cos()),
        }
    }
}

impl ReactivePattern for Spectrum {
    fn render(&mut self, _leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame) {
        // Overall loudness scales the whole field, never below the quiet floor.
        let loud = QUIET_FLOOR + (1.0 - QUIET_FLOOR) * audio.level;
        // Rotate the ripple once per period. Wrapping the clock first keeps f32 exact.
        let w = (t_ms % SWIRL_PERIOD_MS) as f32 / SWIRL_PERIOD_MS as f32 * TAU;
        let (ws, wc) = w.sin_cos();
        let spin = (t_ms % HUE_SPIN_MS) as f32 / HUE_SPIN_MS as f32;

        for (i, o) in out.iter_mut().enumerate() {
            let pos = self.band_pos[i];
            let k = pos as usize;
            let f = pos - k as f32;
            let energy = lerp(audio.bands[k], audio.bands[k + 1], f);
            let hit = lerp(audio.rise[k], audio.rise[k + 1], f);

            // One ripple value drives both the brightness and the color swirl. Being a
            // sinusoid of the angle, it meets itself at the wrap ray with no seam.
            let ripple = self.swirl_s[i] * wc + self.swirl_c[i] * ws;
            let lit = energy * loud * GAIN * (1.0 + SWIRL_DEPTH * ripple);
            let v = (lit + hit * HIT_GAIN).clamp(0.0, 1.0);
            let s = (1.0 - hit * HIT_WHITE).clamp(0.0, 1.0);
            let turns = HUE_BASE_TURN + BAND_TO_TURN * pos + HUE_WOBBLE_TURNS * ripple + spin;
            *o = hsv((turns - turns.floor()) * 360.0, s, v);
        }
    }
}
