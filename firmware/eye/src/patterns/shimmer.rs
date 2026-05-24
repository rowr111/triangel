use super::{Frame, Pattern};
use crate::led::map::{Led, WORLD_BOT, WORLD_CX, WORLD_TOP};
use core::f32::consts::PI;

pub struct CenterShimmer {
    pub speed:      f32, // mm/s outward wave propagation
    pub wavelength: f32, // mm per cycle
}

impl Pattern for CenterShimmer {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        let cy = WORLD_TOP + (WORLD_BOT - WORLD_TOP) / 3.0; // ~149 mm
        let t_s = t_ms as f32 / 1000.0;

        for (i, led) in leds.iter().enumerate() {
            let dist = ((led.wx - WORLD_CX).powi(2) + (led.wy - cy).powi(2)).sqrt();
            let wave = ((dist / self.wavelength - t_s * self.speed / self.wavelength) * PI * 2.0)
                .sin();
            let wave = (wave + 1.0) / 2.0;

            // Per-LED sparkle: deterministic phase offset from board/local index hash
            let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
            let shimmer = 0.6 + 0.4 * (t_ms as f32 * 0.0025 + hash as f32).sin();

            let b = wave * shimmer;
            out[i] = [(b * 160.0) as u8, (b * 210.0) as u8, (b * 255.0) as u8];
        }
    }
}
