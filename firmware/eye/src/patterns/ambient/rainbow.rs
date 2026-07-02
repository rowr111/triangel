use crate::patterns::{Frame, Pattern, hsv};
use crate::led::map::Led;

// Horizontal distance (mm) over which the hue wheel completes one full cycle (~world width).
const HUE_SPAN_MM: f32 = 517.0;

pub struct RainbowX {
    pub speed: f32, // mm/s scroll rate
}

impl Pattern for RainbowX {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        let offset = (t_ms as f32 / 1000.0) * self.speed;
        for (i, led) in leds.iter().enumerate() {
            let hue = ((led.wx + offset) / HUE_SPAN_MM * 360.0).rem_euclid(360.0);
            out[i] = hsv(hue, 1.0, 1.0);
        }
    }
}
