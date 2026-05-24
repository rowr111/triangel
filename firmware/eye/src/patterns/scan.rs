use super::{Frame, Pattern};
use crate::led::map::{Led, WORLD_H, WORLD_TOP};

pub struct HorizontalScan {
    pub period_ms: u32, // full top-to-bottom sweep duration
    pub bandwidth: f32, // mm half-width of the lit band
}

impl Pattern for HorizontalScan {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        let scan_y =
            WORLD_TOP + ((t_ms % self.period_ms) as f32 / self.period_ms as f32) * WORLD_H;
        for (i, led) in leds.iter().enumerate() {
            let brightness = (1.0 - (led.wy - scan_y).abs() / self.bandwidth).max(0.0);
            out[i] = [0, (brightness * 200.0) as u8, (brightness * 255.0) as u8];
        }
    }
}
