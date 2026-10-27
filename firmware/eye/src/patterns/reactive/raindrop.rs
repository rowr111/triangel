use crate::audio::Audio;
use crate::patterns::ripples::{RippleStyle, Ripples};
use crate::patterns::{Frame, Pattern};
use triangel_shared::tuning::raindrop::*;
use triangel_shared::tuning::BASS_BANDS;

const STYLE: RippleStyle = RippleStyle {
    ripple_speed: RIPPLE_SPEED,
    ripple_max_mm: RIPPLE_MAX_MM,
    gain_base: GAIN_BASE,
    gain_hit: GAIN_HIT,
    start_radius_mm: START_RADIUS_MM,
    white_mm: WHITE_MM,
    hit_boost: HIT_BOOST,
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
/// After this many gaps in a row that do not fit, the tempo restarts from the latest.
const BEAT_MISSES: u8 = 4;

/// Rain on black water. Each beat starts drops that send out rings, white while the drop
/// is young and taking on its color as it spreads.
pub struct Raindrop {
    ripples: Ripples,
    armed: bool,
    /// Beat period, or 0 when no tempo has been found.
    beat_ms: f32,
    last_trigger_ms: u32,
    /// Gaps in a row that did not fit the tempo.
    misses: u8,
}

impl Raindrop {
    pub fn new() -> Self {
        Raindrop {
            ripples: Ripples::new(0x9E37_79B9, STYLE),
            armed: true,
            beat_ms: 0.0,
            last_trigger_ms: 0,
            misses: 0,
        }
    }

    /// Updates the tempo estimate from the gap since the last trigger. A gap near double
    /// or half the estimate is halved or doubled first; one that still does not match is
    /// ignored, until BEAT_MISSES in a row restart the tempo.
    fn track_beat(&mut self, t_ms: u32) {
        let since = t_ms.wrapping_sub(self.last_trigger_ms);
        self.last_trigger_ms = t_ms;
        if since > BEAT_LOST_MS {
            self.beat_ms = 0.0;
        }
        let gap = since as f32;
        if !(BEAT_MIN_MS..=BEAT_MAX_MS).contains(&gap) {
            return;
        }
        if self.beat_ms <= 0.0 {
            self.beat_ms = gap;
            self.misses = 0;
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
            self.misses = 0;
        } else {
            self.misses += 1;
            if self.misses >= BEAT_MISSES {
                self.beat_ms = gap;
                self.misses = 0;
            }
        }
    }

    fn land(&mut self, t_ms: u32, strength: f32) {
        let spot = self.ripples.spot();
        let rng = self.ripples.rng();
        let spacing = if self.beat_ms > 0.0 {
            let div = rng.pick(&DIVISIONS);
            (RIPPLE_SPEED * self.beat_ms * div).clamp(SPACING_LOCKED_MIN_MM, SPACING_LOCKED_MAX_MM)
        } else {
            rng.range_f32(SPACING_MIN_MM, SPACING_MAX_MM)
        };
        let count = rng.range_f32(RINGS_MIN, RINGS_MAX);
        self.ripples.land(spot, t_ms, strength, spacing, count);
    }
}

impl Pattern for Raindrop {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        let hit = audio.rise[..BASS_BANDS]
            .iter()
            .copied()
            .fold(0.0f32, f32::max);
        let clear = t_ms.wrapping_sub(self.last_trigger_ms) >= REFRACTORY_MS;
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
