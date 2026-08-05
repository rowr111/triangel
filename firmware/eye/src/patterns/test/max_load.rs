use crate::patterns::{Frame, Pattern};
use crate::led::map::Led;

// MaxLoad - draws the most current the fixture can. Every LED on both chains is held at one
// color; full white on all 600 LEDs is about 36 A at 5 V if each LED pulls its rated 60 mA.
//
// Raise global brightness to the top of the ladder before measuring - the frame is scaled by
// it after rendering, so at the default setting this draws well under full load.

// Color every LED is driven to. White loads all three channels of each LED.
const COLOR: [u8; 3] = [255, 255, 255];

pub struct MaxLoad;

impl Pattern for MaxLoad {
    fn render(&mut self, leds: &[Led], _t_ms: u32, _sound_level: f32, out: &mut Frame) {
        for slot in out.iter_mut().take(leds.len()) {
            *slot = COLOR;
        }
    }
}
