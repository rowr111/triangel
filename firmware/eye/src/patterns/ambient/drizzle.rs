use crate::led::map::Led;
use crate::patterns::ripples::{RippleStyle, Ripples};
use crate::patterns::{Frame, Pattern};

/// Wait between drops, picked fresh each time.
const GAP_MIN_MS: f32 = 600.0;
const GAP_MAX_MS: f32 = 3000.0;
/// Drop strength range. The hardest reach full white.
const STRENGTH_MIN: f32 = 0.35;
const STRENGTH_MAX: f32 = 1.0;
/// Ring spacing in mm, and rings per drop.
const SPACING_MIN_MM: f32 = 24.0;
const SPACING_MAX_MM: f32 = 42.0;
const RINGS_MIN: f32 = 2.0;
const RINGS_MAX: f32 = 5.0;

/// Raindrop's look at half its speed.
const STYLE: RippleStyle = RippleStyle {
    ripple_speed:    0.11,
    ripple_max_mm:   560.0,
    gain_base:       1.2,
    gain_hit:        3.0,
    start_radius_mm: 24.0,
    white_mm:        180.0,
    hit_boost:       4.5,
};

/// Rain on black water with no sound: a drop lands here and there at random and
/// ripples out.
pub struct Drizzle {
    ripples: Ripples,
    last_ms: u32,
    gap_ms:  u32,
}

impl Drizzle {
    pub fn new() -> Self {
        Drizzle { ripples: Ripples::new(0x2545_F491, STYLE), last_ms: 0, gap_ms: 0 }
    }
}

impl Pattern for Drizzle {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        if t_ms.wrapping_sub(self.last_ms) >= self.gap_ms {
            let r = &mut self.ripples;
            let spot = r.spot(leds);
            // Squared so most drops are soft and only a few land hard.
            let hard = r.randf();
            let strength = STRENGTH_MIN + hard * hard * (STRENGTH_MAX - STRENGTH_MIN);
            let spacing = SPACING_MIN_MM + r.randf() * (SPACING_MAX_MM - SPACING_MIN_MM);
            let count = RINGS_MIN + r.randf() * (RINGS_MAX - RINGS_MIN);
            r.land(spot, t_ms, strength, spacing, count);
            self.last_ms = t_ms;
            self.gap_ms = (GAP_MIN_MS + r.randf() * (GAP_MAX_MS - GAP_MIN_MS)) as u32;
        }
        self.ripples.draw(leds, t_ms, out);
    }
}
