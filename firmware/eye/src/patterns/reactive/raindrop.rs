use crate::audio::Audio;
use crate::patterns::ripples::{RippleStyle, Ripples};
use crate::patterns::{Frame, Pattern};
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

/// Ring spacing range once a tempo has been found.
const SPACING_LOCKED_MIN_MM: f32 = 16.0;
const SPACING_LOCKED_MAX_MM: f32 = 150.0;

/// Fractions of a beat a drop can space its rings by.
const DIVISIONS: [f32; 3] = [0.25, 0.5, 1.0];
/// Gaps between triggers outside this range are not counted as beats.
const BEAT_MIN_MS: f32 = 250.0;
const BEAT_MAX_MS: f32 = 1200.0;
/// With no trigger for this long, the tempo is forgotten.
const BEAT_LOST_MS: u32 = 4_000;
/// How far the tempo estimate moves toward each accepted gap.
const BEAT_EASE: f32 = 0.2;

/// Rain on black water. Each beat lands drops that send out rings, the leading edge
/// white and the rings behind it colored.
pub struct Raindrop {
    ripples:      Ripples,
    armed:        bool,
    last_drop_ms: u32,
    /// Beat period, or 0 when no tempo has been found.
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

    /// Updates the tempo estimate from the gap since the last trigger. A gap near double
    /// or half the estimate is halved or doubled first; one that still does not match is
    /// ignored.
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

    fn land(&mut self, t_ms: u32, strength: f32) {
        let spot = self.ripples.spot();
        let rng = self.ripples.rng();
        let spacing = if self.beat_ms > 0.0 {
            let div = rng.pick(&DIVISIONS);
            (RIPPLE_SPEED * self.beat_ms * div)
                .clamp(SPACING_LOCKED_MIN_MM, SPACING_LOCKED_MAX_MM)
        } else {
            SPACING_MIN_MM + rng.f32() * (SPACING_MAX_MM - SPACING_MIN_MM)
        };
        let count = RINGS_MIN + rng.f32() * (RINGS_MAX - RINGS_MIN);
        self.ripples.land(spot, t_ms, strength, spacing, count);
        self.last_drop_ms = t_ms;
    }
}

impl Pattern for Raindrop {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        let hit = audio.rise[..KICK_BANDS].iter().copied().fold(0.0f32, f32::max);
        let clear = t_ms.wrapping_sub(self.last_drop_ms) >= REFRACTORY_MS;
        if self.armed && hit >= TRIGGER && clear {
            self.armed = false;
            let strength = ((hit - TRIGGER) / (1.0 - TRIGGER)).clamp(0.0, 1.0);
            self.track_beat(t_ms);
            let burst = BURST_MIN + (strength * BURST_EXTRA) as usize;
            for _ in 0..burst {
                self.land(t_ms, 0.35 + 0.65 * strength);
            }
        } else if hit < RELEASE {
            self.armed = true;
        }

        if t_ms.wrapping_sub(self.last_trigger_ms) > BEAT_LOST_MS {
            self.beat_ms = 0.0;
        }

        self.ripples.draw(t_ms, out);
    }
}
