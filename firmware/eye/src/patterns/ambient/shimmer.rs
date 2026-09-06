use crate::patterns::{Frame, Pattern, hsv};
use std::sync::OnceLock;

use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, LED_COUNT};
use core::f32::consts::TAU;

// Radial shimmer: a wave ripples out from the center while each LED twinkles on its own
// phase, giving a per-LED brightness field. Color is a vibrant (fully-saturated) hue swept
// across a cool arc by that same field, so the motion and the color share one source.
const DRIFT_PERIOD_MS: u32 = 24_000; // one full hue drift - rotates the bands visibly
const RADIAL_SPAN_MM:  f32 = 100.0;  // distance over which the hue sweeps the arc once
                                     // (lower = more color bands across the fixture at once)
const HASH_JITTER:     f32 = 0.06;   // per-LED hue scatter, for a jeweled twinkle
const SHIMMER_FLOOR:   f32 = 0.10;   // brightness the wave troughs settle at, so it never goes dark
const WAVE_FALLOFF:    f32 = 0.7;    // shapes the crest->trough falloff: <1 broadens the bright crests
                                     // (brighter overall, thin dark troughs); >1 sharpens to punchy peaks

// Vibrant cool hue arc, in degrees. Saturation is full (rainbow-level); the hue ping-pongs
// across [HUE_START, HUE_START + HUE_SPAN] so the bands stay in the cool range and never seam.
const HUE_START:  f32 = 180.0; // cyan
const HUE_SPAN:   f32 = 140.0; // up through blue/violet toward magenta, then back
const SATURATION: f32 = 1.0;   // 1.0 = max vibrance; lower for a softer, brighter look

// Spiral: SPIRAL_ARMS sectors of hue, twisted by radius and spun over time, so they curl
// into arms that pinwheel outward - echoing the triangle's 3-fold shape. Layered on top of
// the radial field. Flip the sign of SPIRAL_TWIST to reverse the winding direction.
const SPIRAL_ARMS:      f32 = 3.0;    // arms around the wheel (3 matches the triangle)
const SPIRAL_TWIST:     f32 = 0.004;  // hue cycles added per mm of radius - how tightly arms wind
const SPIRAL_PERIOD_MS: u32 = 12_000; // one full rotation of the pinwheel

// Distinct values the per-LED hash can take, so its phase and normalized form both table.
const HASH_STEPS: usize = 97;

pub struct CenterShimmer {
    pub speed:      f32, // mm/s outward wave propagation
    pub wavelength: f32, // mm per cycle
    // The radial wave's per-LED phase as a phasor: its argument is a fixed per-LED term plus a
    // per-frame one, so rotating these replaces a sin call per LED.
    w_sin: [f32; LED_COUNT],
    w_cos: [f32; LED_COUNT],
}

impl CenterShimmer {
    pub fn new(speed: f32, wavelength: f32) -> Self {
        let k = TAU / wavelength;
        CenterShimmer {
            speed,
            wavelength,
            w_sin: core::array::from_fn(|i| (DIST_C[i] * k).sin()),
            w_cos: core::array::from_fn(|i| (DIST_C[i] * k).cos()),
        }
    }
}

/// sin, cos and normalized value for every hash the pattern can produce.
fn hash_tables() -> &'static ([f32; HASH_STEPS], [f32; HASH_STEPS], [f32; HASH_STEPS]) {
    static T: OnceLock<([f32; HASH_STEPS], [f32; HASH_STEPS], [f32; HASH_STEPS])> = OnceLock::new();
    T.get_or_init(|| {
        let mut sn = [0.0; HASH_STEPS];
        let mut cs = [0.0; HASH_STEPS];
        let mut norm = [0.0; HASH_STEPS];
        for k in 0..HASH_STEPS {
            let (s, c) = (k as f32).sin_cos();
            sn[k] = s;
            cs[k] = c;
            norm[k] = k as f32 / HASH_STEPS as f32;
        }
        (sn, cs, norm)
    })
}

impl Pattern for CenterShimmer {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        // Fold each time term to its own period before the f32 cast: raw t_ms loses
        // sub-frame precision after hours of uptime. The wave wraps exactly one cycle;
        // sparkle's rounded period leaves a ~0.0007 rad seam every ~2.5 s - invisible.
        const SPARKLE_PERIOD_MS: u32 = 2513; // one sin period at 0.0025 rad/ms (2*pi/0.0025)
        let wave_period_ms = (self.wavelength / self.speed * 1000.0) as u32;
        let t_s = (t_ms % wave_period_ms.max(1)) as f32 / 1000.0;
        let sparkle_t = (t_ms % SPARKLE_PERIOD_MS) as f32;
        // Slow hue drift, folded to its own period for the same long-uptime reason.
        let drift = (t_ms % DRIFT_PERIOD_MS) as f32 / DRIFT_PERIOD_MS as f32;
        // Pinwheel rotation phase (0..1), advancing the spiral arms over time.
        let spin = (t_ms % SPIRAL_PERIOD_MS) as f32 / SPIRAL_PERIOD_MS as f32;

        // Per-frame rotation angles for the wave and the sparkle, each pairing with a per-LED
        // phasor below.
        let (bw_sin, bw_cos) = (-t_s * self.speed * (TAU / self.wavelength)).sin_cos();
        let (bs_sin, bs_cos) = (sparkle_t * 0.0025).sin_cos();
        let (h_sin, h_cos, h_norm) = hash_tables();

        for (i, led) in leds.iter().enumerate() {
            let dist = DIST_C[i];

            // Radial wave (the motion), reshaped by WAVE_FALLOFF so the crest-to-trough
            // falloff broadens the bright band and keeps the dark troughs thin.
            let w = self.w_sin[i] * bw_cos + self.w_cos[i] * bw_sin;
            let wave = ((w + 1.0) / 2.0).powf(WAVE_FALLOFF);

            // Per-LED sparkle: deterministic phase offset from board/local index hash.
            let hash = ((led.board_id as u32 * 7 + led.local_idx as u32 * 13)
                % HASH_STEPS as u32) as usize;
            let shimmer = 0.6 + 0.4 * (h_sin[hash] * bs_cos + h_cos[hash] * bs_sin);

            // Brightness field, lifted off zero so the wave troughs still glow (SHIMMER_FLOOR..1).
            let b = SHIMMER_FLOOR + (1.0 - SHIMMER_FLOOR) * wave * shimmer;

            // Hue position: radial gradient + drift + per-LED scatter, plus a spiral of
            // SPIRAL_ARMS sectors twisted by radius and spun over time (pinwheels outward).
            let hn = h_norm[hash]; // 0..1 per-LED
            let theta = THETA_C[i];
            let spiral = SPIRAL_ARMS * theta / TAU + SPIRAL_TWIST * dist - spin;
            let p = dist / RADIAL_SPAN_MM + drift + (hn - 0.5) * HASH_JITTER + spiral;

            // Ping-pong p into a triangle 0->1->0 so the hue sweeps up the cool arc and
            // back down - concentric bands with no seam where p wraps.
            let tri = 1.0 - (2.0 * p.rem_euclid(1.0) - 1.0).abs();
            let hue = HUE_START + HUE_SPAN * tri;
            out[i] = hsv(hue, SATURATION, b);
        }
    }
}
