use crate::patterns::{Frame, Pattern, hsv};
use crate::led::map::{Led, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use core::f32::consts::{PI, TAU};

// Radial shimmer: a wave ripples out from the center while each LED twinkles on its own
// phase, giving a per-LED brightness field. Color is a vibrant (fully-saturated) hue swept
// across a cool arc by that same field, so the motion and the color share one source.
const DRIFT_PERIOD_MS: u32 = 24_000; // one full hue drift - rotates the bands visibly
const RADIAL_SPAN_MM:  f32 = 100.0;  // distance over which the hue sweeps the arc once
                                     // (lower = more color bands across the fixture at once)
const HASH_JITTER:     f32 = 0.06;   // per-LED hue scatter, for a jeweled twinkle
const SHIMMER_FLOOR:   f32 = 0.10;   // brightness the wave troughs settle at, so it never goes dark

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

pub struct CenterShimmer {
    pub speed:      f32, // mm/s outward wave propagation
    pub wavelength: f32, // mm per cycle
}

impl Pattern for CenterShimmer {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
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

        for (i, led) in leds.iter().enumerate() {
            let dist = led.dist_to(WORLD_CENTROID_X, WORLD_CENTROID_Y);

            // Radial wave (the motion).
            let arg = dist / self.wavelength - t_s * self.speed / self.wavelength;
            let wave = ((arg * PI * 2.0).sin() + 1.0) / 2.0;

            // Per-LED sparkle: deterministic phase offset from board/local index hash.
            let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
            let shimmer = 0.6 + 0.4 * (sparkle_t * 0.0025 + hash as f32).sin();

            // Brightness field, lifted off zero so the wave troughs still glow (SHIMMER_FLOOR..1).
            let b = SHIMMER_FLOOR + (1.0 - SHIMMER_FLOOR) * wave * shimmer;

            // Hue position: radial gradient + drift + per-LED scatter, plus a spiral of
            // SPIRAL_ARMS sectors twisted by radius and spun over time (pinwheels outward).
            let hn = hash as f32 / 97.0; // 0..1 per-LED
            let theta = (led.wy - WORLD_CENTROID_Y).atan2(led.wx - WORLD_CENTROID_X);
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
