use super::{Frame, Pattern};
use crate::led::map::Led;
use core::f32::consts::PI;

pub struct ApexRipple {
    pub speed:      f32, // mm/s outward
    pub wavelength: f32, // mm per cycle
}

impl Pattern for ApexRipple {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        const APEX_X: f32 = 258.0;
        const APEX_Y: f32 = 436.0;
        let t_s = t_ms as f32 / 1000.0;
        for (i, led) in leds.iter().enumerate() {
            let dist = ((led.wx - APEX_X).powi(2) + (led.wy - APEX_Y).powi(2)).sqrt();
            let phase = (dist - t_s * self.speed) / self.wavelength * PI * 2.0;
            let brightness = (phase.sin() + 1.0) / 2.0;
            out[i] = [(brightness * 255.0) as u8, (brightness * 100.0) as u8, 0];
        }
    }
}
