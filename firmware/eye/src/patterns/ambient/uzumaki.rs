use crate::patterns::{Frame, Pattern, hsv, mix_rgb, smoothstep};
use crate::led::geom::{DIST_C, DIST_C_MAX, THETA_C};
use crate::audio::Audio;
use crate::led::map::LED_COUNT;
use core::f32::consts::TAU;

// Uzumaki (spiral): bright lines spiraling out of a glowing core, each trailing a glow of
// its arm's color into black. The bands are spaced by log distance, so they widen as they
// sweep outward.

// Shape and motion
const ARMS:   usize = 3;     // spiral arms, and how many hue families show at once
const ROT_MS: u32   = 6_000; // one full revolution
const R0_MM:  f32   = 40.0;  // radius of the core, where the log scale flattens

const WIND_MID:   f32 = 5.0;    // average number of bands between core and rim
const WIND_SWING: f32 = 1.5;    // how far the winding swings either side of WIND_MID
const BREATH_MS:  u32 = 32_000; // one tighten-and-loosen cycle

const WOBBLE: f32 = 0.15;    // once-around wobble, in band widths (0 = a perfect disc)
const WOB_MS: u32 = 17_000;  // time for the wobble to travel once around

// Keep WIND_SWING * TAU / BREATH_S under ARMS / ROT_S (here 0.29 against 0.50), or the
// outer bands move inward for part of the cycle.

// Band profile, in fractions of one band width
const STRIPE_HALF:   f32 = 0.10; // half-width of the bright line on each band edge
const STRIPE_SOFT:   f32 = 0.05; // softness of that line's edge
const STRIPE_WHITEN: f32 = 0.55; // how far the line goes toward white
// How far the glow reaches from its line. At 0.40 the glows meet and nothing goes dark.
const GLOW_W:    f32 = 0.40;
const GAP_LEVEL: f32 = 0.0;  // brightness left in the gap

// Core
const EYE_FRAC:    f32 = 0.35; // depth the core fades out over
const CORE_WHITEN: f32 = 0.5;  // how far the core goes toward white

// Color: each family is a (hue, sat, val) pair, the deep stop next to the line and the
// bright stop where the glow dims. The arms show three neighboring families at a time,
// so every run of three has to work as a set.
const FAMILIES: usize = 6;

const PALETTE: [[(f32, f32, f32); 2]; FAMILIES] = [
    [(318.0, 0.95, 1.00), (318.0, 0.55, 1.00)], // magenta
    [(272.0, 0.85, 0.95), (272.0, 0.45, 1.00)], // violet
    [(212.0, 0.95, 1.00), (208.0, 0.50, 1.00)], // electric blue
    [(178.0, 0.90, 1.00), (176.0, 0.45, 1.00)], // cyan
    [(100.0, 0.85, 0.95), ( 95.0, 0.45, 1.00)], // lime
    [( 28.0, 0.95, 1.00), ( 35.0, 0.50, 1.00)], // orange
];

const STEP_MS: u32 = 9_000; // how long the arms hold one set of families
const FADE:    f32 = 0.35;  // fraction of that step spent crossfading to the next set

const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

const INV_STRIPE_SOFT: f32 = 1.0 / STRIPE_SOFT;
const INV_GLOW_W:      f32 = 1.0 / GLOW_W;

pub struct Uzumaki {
    // Fixed part of the band phase, in band widths.
    turn0: [f32; LED_COUNT],
    // Log distance from the center: 0 at the core, 1 at the farthest LED.
    depth: [f32; LED_COUNT],
    // sin and cos of the LED's angle, for the wobble.
    wob_s: [f32; LED_COUNT],
    wob_c: [f32; LED_COUNT],
    // How much of the core covers this LED: 1 at the center, 0 outside it.
    eye:   [f32; LED_COUNT],
    // The palette as RGB.
    deep:   [[f32; 3]; FAMILIES],
    bright: [[f32; 3]; FAMILIES],
}

impl Uzumaki {
    pub fn new() -> Self {
        let inv_span = 1.0 / (1.0 + DIST_C_MAX / R0_MM).ln();
        let depth: [f32; LED_COUNT] =
            core::array::from_fn(|i| (1.0 + DIST_C[i] / R0_MM).ln() * inv_span);

        Uzumaki {
            // The added 2 * ARMS keeps the phase positive in render. It and the wrap in
            // THETA_C both shift the phase by a multiple of ARMS, so the colors stay put.
            turn0: core::array::from_fn(|i| ARMS as f32 * THETA_C[i] / TAU + 2.0 * ARMS as f32),
            wob_s: core::array::from_fn(|i| THETA_C[i].sin()),
            wob_c: core::array::from_fn(|i| THETA_C[i].cos()),
            eye:   core::array::from_fn(|i| 1.0 - smoothstep(depth[i] / EYE_FRAC)),
            deep:   core::array::from_fn(|k| stop(PALETTE[k][0])),
            bright: core::array::from_fn(|k| stop(PALETTE[k][1])),
            depth,
        }
    }

    /// This frame's (deep, bright) stops for each arm.
    fn arm_stops(&self, t_ms: u32) -> [([f32; 3], [f32; 3]); ARMS] {
        let cyc = (t_ms % (FAMILIES as u32 * STEP_MS)) as f32 / STEP_MS as f32;
        let idx = cyc as usize;
        let mix = smoothstep((cyc - idx as f32 - (1.0 - FADE)) / FADE);
        core::array::from_fn(|j| {
            let a = (idx + j) % FAMILIES;
            let b = (a + 1) % FAMILIES;
            (mix_rgb(self.deep[a], self.deep[b], mix), mix_rgb(self.bright[a], self.bright[b], mix))
        })
    }
}

impl Pattern for Uzumaki {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let rot = ARMS as f32 * (t_ms % ROT_MS) as f32 / ROT_MS as f32;
        let wind = WIND_MID + WIND_SWING * (TAU * (t_ms % BREATH_MS) as f32 / BREATH_MS as f32).sin();
        let (wob_s, wob_c) = (TAU * (t_ms % WOB_MS) as f32 / WOB_MS as f32).sin_cos();

        // The lines and the core are the average of the arms' bright stops, toward white.
        // The lines need one shared color because each sits between two arms.
        let stops = self.arm_stops(t_ms);
        let avg: [f32; 3] =
            core::array::from_fn(|j| stops.iter().map(|s| s.1[j]).sum::<f32>() / ARMS as f32);
        let stripe_rgb = mix_rgb(avg, WHITE, STRIPE_WHITEN);
        let core_rgb   = mix_rgb(avg, WHITE, CORE_WHITEN);

        for (i, slot) in out.iter_mut().enumerate() {
            let ph = self.turn0[i] + wind * self.depth[i] - rot
                + WOBBLE * (self.wob_s[i] * wob_c - self.wob_c[i] * wob_s);
            let n = ph as u32;
            let f = ph - n as f32;
            let s = f.min(1.0 - f);    // distance to the nearer band edge, 0 to 0.5

            let (deep, bright) = &stops[n as usize % ARMS];
            let glow = 1.0 - smoothstep((s - STRIPE_HALF) * INV_GLOW_W);
            let v = GAP_LEVEL + (1.0 - GAP_LEVEL) * glow;
            let body = mix_rgb(*bright, *deep, glow);
            let st = smoothstep((STRIPE_HALF - s) * INV_STRIPE_SOFT);
            let mut c = mix_rgb([body[0] * v, body[1] * v, body[2] * v], stripe_rgb, st);

            // Near the center the bands are finer than the LED spacing, so fade to the core.
            let e = self.eye[i];
            if e > 0.0 {
                c = mix_rgb(c, core_rgb, e);
            }

            *slot = [c[0] as u8, c[1] as u8, c[2] as u8];
        }
    }
}

/// One palette stop (hue, sat, val) as RGB.
fn stop(c: (f32, f32, f32)) -> [f32; 3] {
    let rgb = hsv(c.0, c.1, c.2);
    [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32]
}
