use crate::audio::Audio;
use crate::led::map::{Led, WORLD_BOT, WORLD_H};
use crate::patterns::{Frame, ReactivePattern};

/// Brightness floor, so a silent room still shows the fixture lit.
const FLOOR: f32 = 0.15;

/// Fills the triangle from the apex upward proportional to sound level, with
/// brightness also scaling with sound level. Loud = more LEDs lit AND brighter.
pub struct AudioFill;

impl ReactivePattern for AudioFill {
    fn render(&mut self, leds: &[Led], _t_ms: u32, audio: &Audio, out: &mut Frame) {
        // level_norm rather than level: the fill tracks the music's own loud and quiet
        // rather than an absolute scale the room rarely spans.
        let fill = audio.level_norm;
        let threshold_y = WORLD_BOT - fill * WORLD_H;
        let brightness = FLOOR + (1.0 - FLOOR) * fill;
        for (i, led) in leds.iter().enumerate() {
            out[i] = if led.wy >= threshold_y {
                [
                    (0.4 * brightness * 255.0) as u8,
                    (0.1 * brightness * 255.0) as u8,
                    (brightness * 255.0) as u8,
                ]
            } else {
                [0, 0, 0]
            };
        }
    }
}
