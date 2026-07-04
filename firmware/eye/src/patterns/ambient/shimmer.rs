use crate::patterns::{Frame, Pattern};
use crate::led::map::{Led, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use core::f32::consts::PI;

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

        for (i, led) in leds.iter().enumerate() {
            let dist = led.dist_to(WORLD_CENTROID_X, WORLD_CENTROID_Y);
            let wave = ((dist / self.wavelength - t_s * self.speed / self.wavelength) * PI * 2.0)
                .sin();
            let wave = (wave + 1.0) / 2.0;

            // Per-LED sparkle: deterministic phase offset from board/local index hash
            let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
            let shimmer = 0.6 + 0.4 * (sparkle_t * 0.0025 + hash as f32).sin();

            let b = wave * shimmer;
            out[i] = [(b * 160.0) as u8, (b * 210.0) as u8, (b * 255.0) as u8];
        }
    }
}
