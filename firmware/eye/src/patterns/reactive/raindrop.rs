use crate::audio::Audio;
use crate::led::map::Led;
use crate::patterns::ripples::{RippleStyle, Ripples};
use crate::patterns::{Frame, ReactivePattern};
use triangel_shared::tuning::raindrop::*;

const STYLE: RippleStyle = RippleStyle {
    ripple_speed:    RIPPLE_SPEED,
    ripple_max_mm:   RIPPLE_MAX_MM,
    gain_base:       GAIN_BASE,
    gain_hit:        GAIN_HIT,
    start_radius_mm: START_RADIUS_MM,
    white_mm:        WHITE_MM,
    hit_boost:       HIT_BOOST,
};

/// Crest spacing is held to this range once a tempo has been found.
const SPACING_LOCKED_MIN_MM: f32 = 16.0;
const SPACING_LOCKED_MAX_MM: f32 = 150.0;

/// Fractions of a beat a drop can space its crests by. Every drop is in time with the
/// music, but one ripples in sixteenths where another ripples in quarters.
const DIVISIONS: [f32; 3] = [0.25, 0.5, 1.0];
/// Gaps outside this range are not beats, and one this long with no beat at all drops
/// back to free-running spacing.
const BEAT_MIN_MS: f32 = 250.0;
const BEAT_MAX_MS: f32 = 1200.0;
const BEAT_LOST_MS: u32 = 4_000;
/// How far the estimate moves toward each accepted gap.
const BEAT_EASE: f32 = 0.2;

/// Rain on black water. Every drop is a beat: it lands and sends out a train of
/// concentric rings, the leading edge white and the rings behind it colored. Nothing
/// but the rings is lit.
pub struct Raindrop {
    ripples:      Ripples,
    armed:        bool,
    last_drop_ms: u32,
    /// Beat period from the gaps between triggers, or 0 when no tempo is established.
    beat_ms:         f32,
    last_trigger_ms: u32,
}

impl Raindrop {
    pub fn new() -> Self {
        Raindrop {
            ripples:      Ripples::new(0x9E37_79B9, STYLE),
            armed:        true,
            last_drop_ms: 0,
            beat_ms:         0.0,
            last_trigger_ms: 0,
        }
    }

    /// Fold a gap that is a double or half of the current estimate back onto it, then
    /// ease the estimate toward it. Gaps that match neither are noise and are dropped.
    fn track_beat(&mut self, t_ms: u32) {
        let gap = t_ms.wrapping_sub(self.last_trigger_ms) as f32;
        self.last_trigger_ms = t_ms;
        if !(BEAT_MIN_MS..=BEAT_MAX_MS).contains(&gap) {
            return;
        }
        if self.beat_ms <= 0.0 {
            self.beat_ms = gap;
            return;
        }
        let mut g = gap;
        if g > self.beat_ms * 1.6 {
            g *= 0.5;
        } else if g < self.beat_ms * 0.65 {
            g *= 2.0;
        }
        if (g - self.beat_ms).abs() < self.beat_ms * 0.25 {
            self.beat_ms += (g - self.beat_ms) * BEAT_EASE;
        }
    }

    /// Land a drop.
    fn land(&mut self, leds: &[Led], t_ms: u32, strength: f32) {
        let spot = self.ripples.spot(leds);
        let spacing = if self.beat_ms > 0.0 {
            let div = DIVISIONS[(self.ripples.randf() * DIVISIONS.len() as f32) as usize % DIVISIONS.len()];
            (RIPPLE_SPEED * self.beat_ms * div)
                .clamp(SPACING_LOCKED_MIN_MM, SPACING_LOCKED_MAX_MM)
        } else {
            SPACING_MIN_MM + self.ripples.randf() * (SPACING_MAX_MM - SPACING_MIN_MM)
        };
        let count = RINGS_MIN + self.ripples.randf() * (RINGS_MAX - RINGS_MIN);
        self.ripples.land(spot, t_ms, strength, spacing, count);
        self.last_drop_ms = t_ms;
    }
}

impl ReactivePattern for Raindrop {
    fn render(&mut self, leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame) {
        let hit = audio.rise[..KICK_BANDS].iter().copied().fold(0.0f32, f32::max);
        let clear = t_ms.wrapping_sub(self.last_drop_ms) >= REFRACTORY_MS;
        if self.armed && hit >= TRIGGER && clear {
            self.armed = false;
            // How far past the trigger the hit reached. The hardest beats land at 1.0,
            // a full white crest.
            let strength = ((hit - TRIGGER) / (1.0 - TRIGGER)).clamp(0.0, 1.0);
            self.track_beat(t_ms);
            let burst = BURST_MIN + (strength * BURST_EXTRA) as usize;
            for _ in 0..burst {
                self.land(leds, t_ms, 0.35 + 0.65 * strength);
            }
        } else if hit < RELEASE {
            self.armed = true;
        }

        // Forget the tempo once the beats stop, so it falls back rather than holding
        // a stale one.
        if t_ms.wrapping_sub(self.last_trigger_ms) > BEAT_LOST_MS {
            self.beat_ms = 0.0;
        }

        self.ripples.draw(leds, t_ms, out);
    }
}
