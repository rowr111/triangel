use crate::audio::Audio;
use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, ReactivePattern, hsv};

// Beat trigger: the strongest onset across bands 0-2, 40-100 Hz. Narrow on purpose -
// wider than this reaches the bass line and low mids, which fire off the beat.
const KICK_BANDS: usize = 3;
// Fires above TRIGGER, and does not re-arm until it falls back under RELEASE, so one
// kick makes one drop.
const TRIGGER: f32 = 0.22;
const RELEASE: f32 = 0.10;
const REFRACTORY_MS: u32 = 120;

/// A ripple lives for several seconds and a hard beat lands several at once, so the
/// pool has to be deep or older ones get evicted while still crossing the fixture.
const MAX_DROPS: usize = 18;
/// Drops a beat lands, scattered across the fixture: every kick gets at least
/// BURST_MIN, and a full-strength one adds BURST_EXTRA on top.
const BURST_MIN: usize = 2;
const BURST_EXTRA: f32 = 2.0;
/// Front travel in mm per ms, and the distance a drop is retired at. The fixture is
/// about 670 mm corner to corner, so this carries a ripple most of the way across.
const RIPPLE_SPEED: f32 = 0.22;
const RIPPLE_MAX_MM: f32 = 560.0;
/// Gain on the crests, scaled by how hard the drop landed. Past 1 they clip to full,
/// so a soft drop stays dim while a hard one blows out into a wide blazing ring.
const GAIN_BASE: f32 = 1.2;
const GAIN_HIT:  f32 = 3.0;
/// Crest spacing when no tempo has been found, and the range it is held to once one
/// has. Each drop picks its own from this range, so no two look alike.
const SPACING_MIN_MM: f32 = 24.0;
const SPACING_MAX_MM: f32 = 42.0;
const SPACING_LOCKED_MIN_MM: f32 = 16.0;
const SPACING_LOCKED_MAX_MM: f32 = 150.0;
const RINGS_MIN: f32 = 2.0;
const RINGS_MAX: f32 = 5.0;

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
/// Radius a drop starts at. LEDs sit about 10 mm apart, so a ripple starting from zero
/// covers nothing for its first frames and the impact goes unseen.
const START_RADIUS_MM: f32 = 24.0;
/// A ripple is white and blown out while it is still small, and takes on its color as
/// it spreads. WHITE_MM is how far the front travels over that change, so it also sets
/// how long the hit reads as the bright part.
const WHITE_MM: f32 = 180.0;
const HIT_BOOST: f32 = 4.5;

/// Hue range drops pick from. Cool and narrow, so overlapping rings can average their
/// hues directly without wrapping past 0.
const HUE_MIN: f32 = 170.0;
const HUE_MAX: f32 = 250.0;
/// Saturation is held back because a fully saturated blue drives only the blue channel
/// and reads dim.
const DROP_SAT: f32 = 0.62;

struct Drop {
    x:        f32,
    y:        f32,
    start_ms: u32,
    strength: f32,
    hue:      f32,
    spacing:  f32,
    /// Distance from the leading edge to the tail of this drop's ring train.
    train:    f32,
    alive:    bool,
}

/// One live drop's geometry for this frame.
#[derive(Clone, Copy)]
struct Ring {
    r:           f32,
    u_inner:     f32,
    u_outer:     f32,
    amp:         f32,
    /// 1 the moment the drop lands, 0 once the front has run WHITE_MM.
    young:       f32,
    inv_spacing: f32,
    inv_train:   f32,
}

/// Rain on black water. Every drop is a beat: it lands and sends out a train of
/// concentric rings, the leading edge white and the rings behind it colored. Nothing
/// but the rings is lit.
pub struct Raindrop {
    rng:          u32,
    drops:        [Drop; MAX_DROPS],
    armed:        bool,
    last_drop_ms: u32,
    /// Beat period from the gaps between triggers, or 0 when no tempo is established.
    beat_ms:         f32,
    last_trigger_ms: u32,
}

impl Raindrop {
    pub fn new() -> Self {
        Raindrop {
            rng:          0x9E37_79B9,
            drops:        core::array::from_fn(|_| Drop {
                x: 0.0, y: 0.0, start_ms: 0, strength: 0.0, hue: HUE_MIN,
                spacing: SPACING_MIN_MM, train: 0.0, alive: false,
            }),
            armed:        true,
            last_drop_ms: 0,
            beat_ms:         0.0,
            last_trigger_ms: 0,
        }
    }

    fn next_rng(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn randf(&mut self) -> f32 {
        (self.next_rng() >> 8) as f32 / 16_777_216.0
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

    /// Land a drop. Its position is a random LED, which keeps it on the fixture and
    /// favors the denser parts of it.
    fn land(&mut self, leds: &[Led], t_ms: u32, strength: f32) {
        let pick = (self.randf() * LED_COUNT as f32) as usize % LED_COUNT;
        let (x, y) = (leds[pick].wx, leds[pick].wy);
        let hue = HUE_MIN + self.randf() * (HUE_MAX - HUE_MIN);
        let spacing = if self.beat_ms > 0.0 {
            let div = DIVISIONS[(self.randf() * DIVISIONS.len() as f32) as usize % DIVISIONS.len()];
            (RIPPLE_SPEED * self.beat_ms * div)
                .clamp(SPACING_LOCKED_MIN_MM, SPACING_LOCKED_MAX_MM)
        } else {
            SPACING_MIN_MM + self.randf() * (SPACING_MAX_MM - SPACING_MIN_MM)
        };
        let count = RINGS_MIN + self.randf() * (RINGS_MAX - RINGS_MIN);
        // Reuse the oldest slot when all are busy, so a strong beat is never dropped.
        let slot = self.drops.iter().position(|d| !d.alive).unwrap_or_else(|| {
            let mut oldest = 0;
            for (i, d) in self.drops.iter().enumerate() {
                if d.start_ms < self.drops[oldest].start_ms {
                    oldest = i;
                }
            }
            oldest
        });
        self.drops[slot] = Drop {
            x,
            y,
            start_ms: t_ms,
            strength,
            hue,
            spacing,
            train: spacing * count,
            alive: true,
        };
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

        let blank = Ring {
            r: 0.0, u_inner: 0.0, u_outer: 0.0, amp: 0.0, young: 0.0,
            inv_spacing: 0.0, inv_train: 0.0,
        };
        let mut rings = [blank; MAX_DROPS];
        for (ring, drop) in rings.iter_mut().zip(self.drops.iter_mut()) {
            if !drop.alive {
                continue;
            }
            let r = START_RADIUS_MM + t_ms.wrapping_sub(drop.start_ms) as f32 * RIPPLE_SPEED;
            if r - drop.train > RIPPLE_MAX_MM {
                drop.alive = false;
                continue;
            }
            let inner = (r - drop.train).max(0.0);
            *ring = Ring {
                r,
                u_inner: inner * inner,
                u_outer: r * r,
                amp: drop.strength * (GAIN_BASE + GAIN_HIT * drop.strength),
                young: (1.0 - (r - START_RADIUS_MM) / WHITE_MM).clamp(0.0, 1.0),
                inv_spacing: 1.0 / drop.spacing,
                inv_train: 1.0 / drop.train,
            };
        }

        let inv_reach = 1.0 / RIPPLE_MAX_MM;

        for (o, led) in out.iter_mut().zip(leds.iter()) {
            let mut ripple = 0.0f32;
            let mut hue_acc = 0.0f32;
            let mut sat_acc = 0.0f32;
            for (ring, drop) in rings.iter().zip(self.drops.iter()) {
                if !drop.alive {
                    continue;
                }
                let dx = led.wx - drop.x;
                let dy = led.wy - drop.y;
                let u = dx * dx + dy * dy;
                // Reject on squared distance, so the square root only runs for the
                // LEDs actually inside this drop's train.
                if u > ring.u_outer || u < ring.u_inner {
                    continue;
                }
                let d = u.sqrt();
                // Distance behind the leading edge: 0 at the front, `train` at the tail.
                let q = ring.r - d;
                // Half a cell of offset puts a crest exactly on the leading edge,
                // rather than a trough.
                let cell = q * ring.inv_spacing + 0.5;
                let frac = cell - cell.floor();
                // Triangle wave, squared for a sharper crest.
                let crest = 1.0 - (2.0 * frac - 1.0).abs();
                let w = ring.amp
                    * crest
                    * crest
                    * (1.0 - q * ring.inv_train)
                    * (1.0 - d * inv_reach).max(0.0)
                    * (1.0 + HIT_BOOST * ring.young);
                ripple += w;
                hue_acc += drop.hue * w;
                sat_acc += DROP_SAT * (1.0 - ring.young) * w;
            }

            if ripple > 1e-3 {
                let inv = 1.0 / ripple;
                *o = hsv(hue_acc * inv, (sat_acc * inv).clamp(0.0, 1.0), ripple.min(1.0));
            } else {
                *o = [0, 0, 0];
            }
        }
    }
}
