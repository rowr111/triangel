use crate::patterns::{Frame, Pattern};
use crate::led::map::Led;

// StepResponse - snaps the whole fixture between two levels with no fade, so the supply sees
// a square-wave load. Watch for LEDs stuttering or flashing the wrong color, or the board
// resetting: all signs the rail is collapsing on the step.
//
// The render loop runs at 30 fps, so times quantize to ~33 ms and the fastest square wave
// available is one frame on, one frame off (about 15 Hz).
//
// Raise global brightness to the top of the ladder before measuring - the frame is scaled by
// it after rendering, so at the default setting the step is a third of its full height.

// Levels the fixture snaps between. LOW black gives the largest step; raise it to test a
// smaller one.
const HIGH: [u8; 3] = [255, 255, 255];
const LOW:  [u8; 3] = [0, 0, 0];

// Fixed rate (SWEEP off): time at HIGH, then time at LOW.
const ON_MS:  u32 = 500;
const OFF_MS: u32 = 500;

// Sweep: ignore ON_MS/OFF_MS and step through these half-periods in ms (1000 = 0.5 Hz,
// 33 = one frame, about 15 Hz), dwelling on each, then repeat. Finds the rate that trips a
// brownout when a single fixed rate does not.
const SWEEP:          bool     = false;
const SWEEP_HALF_MS:  [u32; 6] = [1_000, 500, 250, 125, 66, 33];
const SWEEP_DWELL_MS: u32      = 5_000;

pub struct StepResponse;

impl Pattern for StepResponse {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        let high = if SWEEP { sweep_high(t_ms) } else { fixed_high(t_ms) };
        let color = if high { HIGH } else { LOW };
        for slot in out.iter_mut().take(leds.len()) {
            *slot = color;
        }
    }
}

/// Fixed-rate square wave: HIGH for ON_MS, LOW for OFF_MS.
fn fixed_high(t_ms: u32) -> bool {
    t_ms % (ON_MS + OFF_MS).max(1) < ON_MS
}

/// Swept square wave. Timing restarts at each new half-period so every rate begins on a
/// rising edge rather than mid-pulse.
fn sweep_high(t_ms: u32) -> bool {
    let phase = t_ms % (SWEEP_DWELL_MS * SWEEP_HALF_MS.len() as u32).max(1);
    let half  = SWEEP_HALF_MS[(phase / SWEEP_DWELL_MS.max(1)) as usize];
    let local = phase % SWEEP_DWELL_MS.max(1);
    local % (half * 2).max(1) < half
}
