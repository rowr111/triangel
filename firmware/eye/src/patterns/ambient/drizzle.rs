use crate::audio::Audio;
use crate::patterns::ripples::{RippleStyle, Ripples};
use crate::patterns::{Frame, Pattern};
use triangel_shared::tuning::raindrop::{
    GAIN_BASE, GAIN_HIT, HIT_BOOST, RINGS_MAX, RINGS_MIN, RIPPLE_MAX_MM, RIPPLE_SPEED,
    SPACING_MAX_MM, SPACING_MIN_MM, START_RADIUS_MM, WHITE_MM,
};

/// Wait between drops.
const GAP_MIN_MS: f32 = 600.0;
const GAP_MAX_MS: f32 = 3000.0;
/// Drop strength range.
const STRENGTH_MIN: f32 = 0.35;
const STRENGTH_MAX: f32 = 1.0;

/// Raindrop's look at half its speed.
const STYLE: RippleStyle = RippleStyle {
    ripple_speed: RIPPLE_SPEED * 0.5,
    ripple_max_mm: RIPPLE_MAX_MM,
    gain_base: GAIN_BASE,
    gain_hit: GAIN_HIT,
    start_radius_mm: START_RADIUS_MM,
    white_mm: WHITE_MM,
    hit_boost: HIT_BOOST,
};

/// Rain on black water: drops land at random spots and times and ripple out.
pub struct Drizzle {
    ripples: Ripples,
    last_drop_ms: u32,
    gap_ms: u32,
}

impl Drizzle {
    pub fn new() -> Self {
        Drizzle {
            ripples: Ripples::new(0x2545_F491, STYLE),
            last_drop_ms: 0,
            gap_ms: 0,
        }
    }
}

impl Pattern for Drizzle {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        if t_ms.wrapping_sub(self.last_drop_ms) >= self.gap_ms {
            let r = &mut self.ripples;
            let spot = r.spot();
            // Squared, so most drops are soft.
            let hard = r.rng().f32();
            let strength = STRENGTH_MIN + hard * hard * (STRENGTH_MAX - STRENGTH_MIN);
            let spacing = r.rng().range_f32(SPACING_MIN_MM, SPACING_MAX_MM);
            let count = r.rng().range_f32(RINGS_MIN, RINGS_MAX);
            r.land(spot, t_ms, strength, spacing, count);
            self.last_drop_ms = t_ms;
            self.gap_ms = r.rng().range_f32(GAP_MIN_MS, GAP_MAX_MS) as u32;
        }
        self.ripples.draw(t_ms, out);
    }
}
