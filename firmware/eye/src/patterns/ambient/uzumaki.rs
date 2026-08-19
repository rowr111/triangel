use crate::patterns::{Frame, Pattern, hsv};
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, LED_COUNT};
use core::f32::consts::TAU;

// Uzumaki (spiral) - a hypnotist's disc in 80s airbrush colors: pale arms winding out of a
// glowing core behind a lattice of bright lines, turning steadily so the fixture reads as an
// endless outward rush. The arms are laid out in log distance, so each band is a fixed ratio
// wider than the one inside it and keeps growing as it sweeps out - that is what makes the
// expansion look infinite rather than periodic. Each band is a bright line at its edge with
// the arm's color glowing off it, falling away through the deeper stop into black by the
// middle of the band - so the lit lattice sits on real empty space, not on a full wash.
//
// Nothing repeats exactly: the winding tightens and loosens on its own slow cycle (which
// speeds up and eases the outward rush with it), a once-around wobble precesses so the disc
// is never quite round, and the three arms slide through the six hue families together.
//
// Stateless - a frame is a function of t_ms alone, so there is no re-entry handling. The
// per-LED geometry (log depth, fixed angular phase, wobble phasor, core fade) is built once
// in new(), leaving the hot loop trig-free: two multiply-adds for the phase, then arithmetic.

// ============================== Tuning knobs ==============================

// ---- Shape and motion ----
const ARMS:   usize = 3;     // spiral arms - also how many hue families show at once
const ROT_MS: u32   = 6_000; // time for the arms to make one full revolution
const R0_MM:  f32   = 40.0;  // radius the log scale flattens inside - the size of the core

const WIND_MID:   f32 = 5.0;    // bands between core and rim, on average - the line count
const WIND_SWING: f32 = 1.5;    // how far the winding breathes either side of WIND_MID
const BREATH_MS:  u32 = 32_000; // one tighten-and-loosen cycle

const WOBBLE: f32 = 0.15;    // once-around phase wobble, in band widths (0 = a perfect disc)
const WOB_MS: u32 = 17_000;  // time for the wobble's high side to travel once around

// The bands only ever sweep outward while the winding breathes slower than the arms turn:
// keep WIND_SWING * TAU / BREATH_S under ARMS / ROT_S (here 0.29 against 0.50), or a
// tightening spiral drags the outer bands back inward for part of the cycle.

// ---- Band profile, in fractions of one band width ----
const STRIPE_HALF:   f32 = 0.10; // half-width of the bright line straddling each band edge
const STRIPE_SOFT:   f32 = 0.05; // how soft that line's edge is (small = crisp)
const STRIPE_WHITEN: f32 = 0.55; // how far the line washes toward white from the arms' colors
// How far the arm's color glows off its line before reaching the empty middle of the band.
// This is the emptiness knob: at 0.40 the glow just meets in the middle and nothing goes dark,
// and the lower it goes the more black sits between the lines.
const GLOW_W:    f32 = 0.40;
const GAP_LEVEL: f32 = 0.0;  // brightness left in that gap (0 = true black, not a dim wash)

// ---- Core ----
const EYE_FRAC:    f32 = 0.35; // depth the core fades out over (bands are unresolvable inside it)
const CORE_WHITEN: f32 = 0.5;  // how far the core washes toward white from the arms' own colors

// ---- Color ----
// A palette is six hue families, each a (hue, sat, val) pair: the deep stop carries the lit
// halo beside the line, the bright stop takes over as that halo dims away into the gap. The
// arms take three neighboring families and slide along together, so all six come around in
// turn - the order matters, since every run of three neighbors has to work as a set.
const FAMILIES: usize = 6;

// 80s airbrush, light and chalky.
#[allow(dead_code)] // whichever palette PALETTE is not pointing at
const TRAPPER_KEEPER: [[(f32, f32, f32); 2]; FAMILIES] = [
    [(355.0, 0.53, 0.97), (356.0, 0.34, 0.98)], // coral    -> pink
    [( 93.0, 0.59, 0.68), ( 93.0, 0.36, 0.80)], // sage     -> light green
    [( 40.0, 0.90, 0.82), ( 40.0, 0.63, 0.94)], // amber    -> gold
    [(207.0, 0.66, 0.91), (208.0, 0.41, 0.95)], // sky blue -> pale blue
    [(286.0, 0.42, 0.89), (286.0, 0.27, 0.93)], // lavender -> lilac
    [(171.0, 1.00, 0.70), (171.0, 0.85, 1.00)], // teal     -> electric cyan
];

// The same shape read as neon tube on black: saturated where it is lit, so the hue survives
// on the strip instead of washing to white, and every stop near full brightness.
#[allow(dead_code)]
const NEON: [[(f32, f32, f32); 2]; FAMILIES] = [
    [(318.0, 0.95, 1.00), (318.0, 0.55, 1.00)], // magenta
    [(272.0, 0.85, 0.95), (272.0, 0.45, 1.00)], // violet
    [(212.0, 0.95, 1.00), (208.0, 0.50, 1.00)], // electric blue
    [(178.0, 0.90, 1.00), (176.0, 0.45, 1.00)], // cyan
    [(100.0, 0.85, 0.95), ( 95.0, 0.45, 1.00)], // lime
    [( 28.0, 0.95, 1.00), ( 35.0, 0.50, 1.00)], // orange
];

// The palette in use - swap the right-hand side to audition the other one.
const PALETTE: [[(f32, f32, f32); 2]; FAMILIES] = NEON;
const STEP_MS: u32 = 9_000; // how long the arms hold one set of families
const FADE:    f32 = 0.35;  // fraction of that step spent crossfading to the next set

const WHITE: [f32; 3] = [255.0, 255.0, 255.0];

// Reciprocals of the profile widths, so the hot loop multiplies instead of dividing.
const INV_STRIPE_SOFT: f32 = 1.0 / STRIPE_SOFT;
const INV_GLOW_W:      f32 = 1.0 / GLOW_W;

pub struct Uzumaki {
    // Fixed part of the band phase (the arms' twist around the center), in band widths.
    turn0: [f32; LED_COUNT],
    // Log distance from the center, 0 at the core, 1 at the farthest LED.
    depth: [f32; LED_COUNT],
    // sin/cos of the LED's angle, paired with the frame's angle to rotate the wobble.
    wob_s: [f32; LED_COUNT],
    wob_c: [f32; LED_COUNT],
    // How much of the flat core this LED is under: 1 at the very center, 0 outside it.
    eye:   [f32; LED_COUNT],
    // The palette resolved to RGB once, since the families themselves never change.
    deep:   [[f32; 3]; FAMILIES],
    bright: [[f32; 3]; FAMILIES],
}

impl Uzumaki {
    pub fn new() -> Self {
        // Log depth: bands laid out on it are a fixed ratio wider than the one inside them,
        // so they grow as they sweep out. R0_MM flattens the curve near the center, where
        // otherwise the bands would pack in tighter than the LEDs can show.
        let rmax = DIST_C.iter().fold(0.0f32, |m, &d| m.max(d));
        let inv_span = 1.0 / (1.0 + rmax / R0_MM).ln();
        let depth: [f32; LED_COUNT] =
            core::array::from_fn(|i| (1.0 + DIST_C[i] / R0_MM).ln() * inv_span);

        Uzumaki {
            // The whole-turn offset keeps the phase positive in render, so the band index is
            // a truncating cast rather than a floor. A multiple of ARMS leaves the arm's
            // color untouched, as does the wrap in THETA_C: crossing it steps the phase by
            // exactly ARMS bands, so the spiral has no seam.
            turn0: core::array::from_fn(|i| ARMS as f32 * THETA_C[i] / TAU + 2.0 * ARMS as f32),
            wob_s: core::array::from_fn(|i| THETA_C[i].sin()),
            wob_c: core::array::from_fn(|i| THETA_C[i].cos()),
            eye:   core::array::from_fn(|i| 1.0 - smoothstep((depth[i] / EYE_FRAC).min(1.0))),
            deep:   core::array::from_fn(|k| stop(PALETTE[k][0])),
            bright: core::array::from_fn(|k| stop(PALETTE[k][1])),
            depth,
        }
    }

    /// This frame's (deep, bright) stops for each arm: three neighboring families, sliding
    /// one along every STEP_MS and crossfading over the last FADE of the step.
    fn arm_stops(&self, t_ms: u32) -> [([f32; 3], [f32; 3]); ARMS] {
        let cyc = (t_ms % (FAMILIES as u32 * STEP_MS)) as f32 / STEP_MS as f32;
        let idx = cyc as usize;
        let mix = smoothstep(((cyc - idx as f32 - (1.0 - FADE)) / FADE).clamp(0.0, 1.0));
        core::array::from_fn(|j| {
            let a = (idx + j) % FAMILIES;
            let b = (a + 1) % FAMILIES;
            (mix_rgb(self.deep[a], self.deep[b], mix), mix_rgb(self.bright[a], self.bright[b], mix))
        })
    }
}

impl Pattern for Uzumaki {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        // One revolution advances the phase by exactly ARMS bands, so folding time at ROT_MS
        // is exact: it shifts the band index by a whole ARMS and leaves the colors in place.
        let rot = ARMS as f32 * (t_ms % ROT_MS) as f32 / ROT_MS as f32;

        // The winding breathes, which also speeds up and eases the outward rush (bands travel
        // at rotation over winding).
        let wind = WIND_MID + WIND_SWING * (TAU * (t_ms % BREATH_MS) as f32 / BREATH_MS as f32).sin();

        // Wobble: a once-around offset whose high side travels around over WOB_MS. Rotating
        // each LED's stored phasor by the frame's angle costs two multiplies, not a sin.
        let (wob_s, wob_c) = (TAU * (t_ms % WOB_MS) as f32 / WOB_MS as f32).sin_cos();

        // The lines and the core both take the average of the arms' bright stops washed toward
        // white, so they sit in the palette the disc is currently wearing. The lines have to be
        // one color for every band: each straddles the boundary between two arms.
        let stops = self.arm_stops(t_ms);
        let avg: [f32; 3] =
            core::array::from_fn(|j| stops.iter().map(|s| s.1[j]).sum::<f32>() / ARMS as f32);
        let stripe_rgb = mix_rgb(avg, WHITE, STRIPE_WHITEN);
        let core_rgb   = mix_rgb(avg, WHITE, CORE_WHITEN);

        for (i, slot) in out.iter_mut().take(leds.len()).enumerate() {
            let ph = self.turn0[i] + wind * self.depth[i] - rot
                + WOBBLE * (self.wob_s[i] * wob_c - self.wob_c[i] * wob_s);
            let n = ph as u32;         // floor: turn0's offset keeps ph positive
            let f = ph - n as f32;
            let s = f.min(1.0 - f);    // distance to the nearer band edge, 0 to 0.5

            // A bright line straddles each band edge; the arm's color glows off it, saturated
            // where the halo is lit and easing to the paler stop as it dims into the gap. The
            // deep stop goes next to the line because that is the part actually seen lit.
            let (deep, bright) = &stops[n as usize % ARMS];
            let glow = 1.0 - smoothstep(((s - STRIPE_HALF) * INV_GLOW_W).clamp(0.0, 1.0));
            let v = GAP_LEVEL + (1.0 - GAP_LEVEL) * glow;
            let body = mix_rgb(*bright, *deep, glow);
            let st = smoothstep(((STRIPE_HALF - s) * INV_STRIPE_SOFT).clamp(0.0, 1.0));
            let mut c = mix_rgb([body[0] * v, body[1] * v, body[2] * v], stripe_rgb, st);

            // Near the center the bands are finer than the LED spacing, so fade them into the
            // flat core the arms appear to wind out of.
            let e = self.eye[i];
            if e > 0.0 {
                c = mix_rgb(c, core_rgb, e);
            }

            *slot = [c[0] as u8, c[1] as u8, c[2] as u8];
        }
    }
}

/// One palette stop (hue, sat, val) as RGB, held as f32 for mixing.
fn stop(c: (f32, f32, f32)) -> [f32; 3] {
    let rgb = hsv(c.0, c.1, c.2);
    [rgb[0] as f32, rgb[1] as f32, rgb[2] as f32]
}

fn mix_rgb(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Smooth 0-1 ramp. Expects `t` already clamped to 0-1.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}
