use super::{Frame, Pattern};
use crate::led::map::{Led, WORLD_BOT, WORLD_H, WORLD_TOP};

/// Fills the triangle from the apex upward proportional to sound level, with
/// brightness also scaling with sound level. Loud = more LEDs lit AND brighter.
pub struct AudioFill;

impl Pattern for AudioFill {
    fn render(&mut self, leds: &[Led], _t_ms: u32, sound_level: f32, out: &mut Frame) {
        let threshold_y = WORLD_BOT - sound_level * WORLD_H;
        let brightness  = sound_level;
        for (i, led) in leds.iter().enumerate() {
            if led.wy >= threshold_y {
                out[i] = [
                    (0.4 * brightness * 255.0) as u8,
                    (0.1 * brightness * 255.0) as u8,
                    (brightness * 255.0) as u8,
                ];
            } else {
                out[i] = [0, 0, 0];
            }
        }

        // debug: top-left corner LED always red so we can confirm frames are live
        if let Some(i) = leds.iter().position(|l| l.wy <= WORLD_TOP + 1.0 && l.wx < 15.0) {
            out[i] = [255, 0, 0];
        }
    }
}
