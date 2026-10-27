use core::f32::consts::TAU;

use crate::audio::Audio;
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::LED_COUNT;
use crate::patterns::{cycle, lerp, Frame, Pattern, Phasor};
use triangel_shared::frame::BAND_COUNT;
use triangel_shared::tuning::spectrum::*;

// Color ramps from the core to the rim. Every stop has its brightest channel at 255, so
// brightness is set only by `brightness`.
const STOPS: usize = 5;
#[rustfmt::skip]
const PALETTES: [[[f32; 3]; STOPS]; 6] = [
    // Ember: red through orange and gold to a blue-white core.
    [[255.0, 28.0, 0.0], [255.0, 90.0, 0.0], [255.0, 190.0, 40.0], [255.0, 240.0, 200.0],
     [170.0, 210.0, 255.0]],
    // Cool arc: cyan up through blue and violet to magenta.
    [[0.0, 255.0, 255.0], [0.0, 150.0, 255.0], [90.0, 90.0, 255.0], [180.0, 70.0, 255.0],
     [255.0, 70.0, 205.0]],
    // Aurora: green through teal and cyan to white.
    [[60.0, 255.0, 120.0], [0.0, 255.0, 195.0], [0.0, 233.0, 255.0], [150.0, 215.0, 255.0],
     [240.0, 250.0, 255.0]],
    // Sunset: magenta through rose and orange to cream.
    [[255.0, 45.0, 140.0], [255.0, 85.0, 95.0], [255.0, 145.0, 45.0], [255.0, 205.0, 65.0],
     [255.0, 240.0, 195.0]],
    // Teal to rose, through cream.
    [[0.0, 255.0, 248.0], [128.0, 255.0, 238.0], [255.0, 245.0, 198.0], [255.0, 170.0, 80.0],
     [255.0, 110.0, 140.0]],
    // Violet, from deep to white.
    [[146.0, 55.0, 255.0], [163.0, 76.0, 255.0], [190.0, 110.0, 255.0], [225.0, 180.0, 255.0],
     [250.0, 240.0, 255.0]],
];
/// Maps a band index onto the ramp.
const BAND_TO_RAMP: f32 = 1.0 / (BAND_COUNT - 1) as f32;

// Lobes in the slow swirl that rotates around the center.
const LOBES: f32 = 3.0;

/// The bands by distance from the center: bass at the core, treble at the rim. LEDs are
/// ranked by distance and spread evenly from the first band to the last.
pub struct Spectrum {
    /// Fractional band index per LED, 0.0 to BAND_COUNT-1.
    band_pos: [f32; LED_COUNT],
    /// The LED's angle in the swirl, LOBES * theta.
    swirl_angle: [Phasor; LED_COUNT],
}

impl Spectrum {
    pub fn new() -> Self {
        let mut order: [usize; LED_COUNT] = core::array::from_fn(|i| i);
        order.sort_unstable_by(|&a, &b| DIST_C[a].total_cmp(&DIST_C[b]));
        let mut band_pos = [0.0f32; LED_COUNT];
        let span = (BAND_COUNT - 1) as f32 / LED_COUNT as f32;
        for (rank, &i) in order.iter().enumerate() {
            band_pos[i] = rank as f32 * span;
        }
        Spectrum {
            band_pos,
            swirl_angle: core::array::from_fn(|i| Phasor::new(LOBES * THETA_C[i])),
        }
    }
}

impl Pattern for Spectrum {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        let loud = QUIET_FLOOR + (1.0 - QUIET_FLOOR) * audio.level_norm;
        let swirl_turn = Phasor::new(cycle(t_ms, SWIRL_PERIOD_MS) * TAU);
        let flow = cycle(t_ms, FLOW_PERIOD_MS) * 2.0;
        let (slot, front) = palette_change(t_ms);
        let (from, to) = (&PALETTES[slot], &PALETTES[(slot + 1) % PALETTES.len()]);
        let inv_edge = 1.0 / FRONT_EDGE;

        for (i, px) in out.iter_mut().enumerate() {
            let pos = self.band_pos[i];
            let band = pos as usize;
            let frac = pos - band as f32;
            let energy = lerp(audio.bands[band], audio.bands[band + 1], frac);
            let hit = lerp(audio.rise[band], audio.rise[band + 1], frac);

            let swirl = self.swirl_angle[i].sin_plus(swirl_turn);
            let lit = energy * loud * GAIN * (1.0 + SWIRL_DEPTH * swirl);
            let brightness = (lit + hit * HIT_GAIN).clamp(0.0, 1.0);

            let (seg, stop_frac) = ramp_stop(pos, flow, swirl);
            let white = hit * HIT_WHITE;
            // 1 where the new palette has arrived, 0 where it has not.
            let arrived = ((front - pos) * inv_edge).clamp(0.0, 1.0);
            for (channel, value) in px.iter_mut().enumerate() {
                let old = lerp(from[seg][channel], from[seg + 1][channel], stop_frac);
                let color = if arrived <= 0.0 {
                    old
                } else {
                    let new = lerp(to[seg][channel], to[seg + 1][channel], stop_frac);
                    lerp(old, new, arrived)
                };
                *value = (lerp(color, 255.0, white) * brightness) as u8;
            }
        }
    }
}

/// Which palette is showing, and how far the change to the next one has spread.
fn palette_change(t_ms: u32) -> (usize, f32) {
    let slot = (t_ms / PALETTE_MS) as usize % PALETTES.len();
    let within = t_ms % PALETTE_MS;
    let held = PALETTE_MS - PALETTE_FADE_MS;
    let mix = if within > held {
        (within - held) as f32 / PALETTE_FADE_MS as f32
    } else {
        0.0
    };
    // How far the palette change has spread, in band positions; it starts and ends
    // past the ends.
    let front = mix * ((BAND_COUNT - 1) as f32 + 2.0 * FRONT_EDGE) - FRONT_EDGE;
    (slot, front)
}

/// The ramp stop below an LED's color, and how far it is toward the next stop.
fn ramp_stop(pos: f32, flow: f32, swirl: f32) -> (usize, f32) {
    // Position along the ramp. Subtracting the flow moves colors outward; the 4.0
    // keeps it positive.
    let mut ramp_pos = pos * BAND_TO_RAMP - flow + SWIRL_SHIFT * swirl + 4.0;
    // Fold into 0..2 and reflect the upper half, so the ramp runs out and back.
    ramp_pos -= 2.0 * ((ramp_pos * 0.5) as u32) as f32;
    let stop_pos = ramp_pos.min(2.0 - ramp_pos) * (STOPS - 1) as f32;
    let seg = (stop_pos as usize).min(STOPS - 2);
    (seg, stop_pos - seg as f32)
}
