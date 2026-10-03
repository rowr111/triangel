use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, hsv};

/// A ripple lives for several seconds and a hard beat lands several at once, so the
/// pool has to be deep or older ones get evicted while still crossing the fixture.
const MAX_DROPS: usize = 18;

/// Hue range drops pick from. Cool and narrow, so overlapping rings can average their
/// hues directly without wrapping past 0.
const HUE_MIN: f32 = 170.0;
const HUE_MAX: f32 = 250.0;
/// Saturation is held back because a fully saturated blue drives only the blue channel
/// and reads dim.
const DROP_SAT: f32 = 0.62;

/// How a pattern's ripples look.
#[derive(Clone, Copy)]
pub struct RippleStyle {
    /// Ripple speed in mm per ms, and how far one travels before it is gone.
    pub ripple_speed:    f32,
    pub ripple_max_mm:   f32,
    /// Brightness of a soft drop, and how much more a hard one gets.
    pub gain_base:       f32,
    pub gain_hit:        f32,
    /// Size a drop starts at, in mm.
    pub start_radius_mm: f32,
    /// How far a ripple travels while still white, and how much brighter it is then.
    pub white_mm:        f32,
    pub hit_boost:       f32,
}

/// Where a drop lands, and its hue.
#[derive(Clone, Copy)]
pub struct Spot {
    x:   f32,
    y:   f32,
    hue: f32,
}

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
    x:           f32,
    y:           f32,
    r:           f32,
    u_inner:     f32,
    u_outer:     f32,
    /// Already carries the drop's gain and its young boost.
    amp:         f32,
    hue:         f32,
    /// Already carries the palette saturation and the fade out of white.
    sat_w:       f32,
    inv_spacing: f32,
    inv_train:   f32,
}

/// Drops rippling out across black water, shared by Raindrop and Drizzle. The pattern
/// decides when a drop lands; this draws it.
pub struct Ripples {
    rng:   u32,
    style: RippleStyle,
    drops: [Drop; MAX_DROPS],
}

impl Ripples {
    pub fn new(seed: u32, style: RippleStyle) -> Self {
        Ripples {
            rng: seed,
            style,
            drops: core::array::from_fn(|_| Drop {
                x: 0.0, y: 0.0, start_ms: 0, strength: 0.0, hue: HUE_MIN,
                spacing: 0.0, train: 0.0, alive: false,
            }),
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

    pub fn randf(&mut self) -> f32 {
        (self.next_rng() >> 8) as f32 / 16_777_216.0
    }

    /// Pick where the next drop lands. Its position is a random LED, which keeps it on
    /// the fixture and favors the denser parts of it.
    pub fn spot(&mut self, leds: &[Led]) -> Spot {
        let pick = (self.randf() * LED_COUNT as f32) as usize % LED_COUNT;
        let (x, y) = (leds[pick].wx, leds[pick].wy);
        let hue = HUE_MIN + self.randf() * (HUE_MAX - HUE_MIN);
        Spot { x, y, hue }
    }

    /// Land a drop at `spot` with `count` rings `spacing` mm apart.
    pub fn land(&mut self, spot: Spot, t_ms: u32, strength: f32, spacing: f32, count: f32) {
        // Reuse the oldest slot when all are busy, so a new drop is never lost.
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
            x: spot.x,
            y: spot.y,
            start_ms: t_ms,
            strength,
            hue: spot.hue,
            spacing,
            train: spacing * count,
            alive: true,
        };
    }

    pub fn draw(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        let s = self.style;
        let blank = Ring {
            x: 0.0, y: 0.0, r: 0.0, u_inner: 0.0, u_outer: 0.0, amp: 0.0, hue: 0.0,
            sat_w: 0.0, inv_spacing: 0.0, inv_train: 0.0,
        };
        // Gather only the live drops, so the per-LED loop walks one tight array with no
        // liveness check and no second lookup.
        let mut live = [blank; MAX_DROPS];
        let mut n_live = 0;
        for drop in self.drops.iter_mut() {
            if !drop.alive {
                continue;
            }
            let r = s.start_radius_mm + t_ms.wrapping_sub(drop.start_ms) as f32 * s.ripple_speed;
            if r - drop.train > s.ripple_max_mm {
                drop.alive = false;
                continue;
            }
            let inner = (r - drop.train).max(0.0);
            let young = (1.0 - (r - s.start_radius_mm) / s.white_mm).clamp(0.0, 1.0);
            live[n_live] = Ring {
                x: drop.x,
                y: drop.y,
                r,
                u_inner: inner * inner,
                u_outer: r * r,
                // The young boost and the palette saturation fold in here rather than
                // being multiplied again for every LED.
                amp: drop.strength
                    * (s.gain_base + s.gain_hit * drop.strength)
                    * (1.0 + s.hit_boost * young),
                hue: drop.hue,
                sat_w: DROP_SAT * (1.0 - young),
                inv_spacing: 1.0 / drop.spacing,
                inv_train: 1.0 / drop.train,
            };
            n_live += 1;
        }

        let inv_reach = 1.0 / s.ripple_max_mm;

        for (o, led) in out.iter_mut().zip(leds.iter()) {
            let mut ripple = 0.0f32;
            let mut hue_acc = 0.0f32;
            let mut sat_acc = 0.0f32;
            for ring in live[..n_live].iter() {
                let dx = led.wx - ring.x;
                let dy = led.wy - ring.y;
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
                // The reject above leaves q at zero or more, so truncating is the same
                // as flooring and far cheaper on a chip with no floating-point unit.
                let frac = cell - (cell as u32) as f32;
                // Triangle wave, squared for a sharper crest.
                let crest = 1.0 - (2.0 * frac - 1.0).abs();
                let w = ring.amp
                    * crest
                    * crest
                    * (1.0 - q * ring.inv_train)
                    * (1.0 - d * inv_reach).max(0.0);
                ripple += w;
                hue_acc += ring.hue * w;
                sat_acc += ring.sat_w * w;
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
