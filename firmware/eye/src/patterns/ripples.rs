use crate::led::map::{LED_COUNT, LED_MAP};
use crate::patterns::{hsv, Frame, Rng};

/// Enough that a drop is rarely replaced while its ripple is still crossing the fixture.
const MAX_DROPS: usize = 18;

/// Hue range for drops. Narrow, so overlapping rings can average their hues directly.
const HUE_MIN: f32 = 170.0;
const HUE_MAX: f32 = 250.0;
/// Below 1 because a fully saturated blue lights only the blue channel and looks dim.
const DROP_SAT: f32 = 0.62;

/// How a pattern's ripples look.
#[derive(Clone, Copy)]
pub struct RippleStyle {
    /// Ripple speed in mm per ms, and how far one travels before it is gone.
    pub ripple_speed: f32,
    pub ripple_max_mm: f32,
    /// Brightness of a soft drop, and how much more a hard one gets.
    pub gain_base: f32,
    pub gain_hit: f32,
    /// Size a drop starts at, in mm.
    pub start_radius_mm: f32,
    /// How far a ripple travels while still white, and how much brighter it is then.
    pub white_mm: f32,
    pub hit_boost: f32,
}

/// Where a drop lands, and its hue.
#[derive(Clone, Copy)]
pub struct Spot {
    x: f32,
    y: f32,
    hue: f32,
}

struct Droplet {
    x: f32,
    y: f32,
    start_ms: u32,
    strength: f32,
    hue: f32,
    spacing: f32,
    /// Distance from the leading edge back to the last ring.
    train: f32,
    alive: bool,
}

impl Droplet {
    /// Leading-edge radius, how young the drop is (1 to 0), and ring brightness at `t_ms`.
    fn front(&self, t_ms: u32, s: &RippleStyle) -> (f32, f32, f32) {
        let r = s.start_radius_mm + t_ms.wrapping_sub(self.start_ms) as f32 * s.ripple_speed;
        let young = (1.0 - (r - s.start_radius_mm) / s.white_mm).clamp(0.0, 1.0);
        let amp = self.strength
            * (s.gain_base + s.gain_hit * self.strength)
            * (1.0 + s.hit_boost * young);
        (r, young, amp)
    }

    /// Roughly how bright the brightest point of the rings is at `t_ms`.
    fn glow(&self, t_ms: u32, s: &RippleStyle) -> f32 {
        let (r, _, amp) = self.front(t_ms, s);
        let inner = (r - self.train).max(0.0);
        let at = ((s.ripple_max_mm + r - self.train) * 0.5)
            .max(inner)
            .min(r.min(s.ripple_max_mm));
        amp * (1.0 - (r - at) / self.train) * (1.0 - at / s.ripple_max_mm)
    }
}

/// One live drop's geometry for this frame.
#[derive(Clone, Copy)]
struct Ring {
    x: f32,
    y: f32,
    r: f32,
    u_inner: f32,
    u_outer: f32,
    /// Includes the drop's gain and the boost while it is young.
    amp: f32,
    hue: f32,
    /// Saturation: 0 while the ripple is still white, rising to DROP_SAT.
    sat_w: f32,
    inv_spacing: f32,
    inv_train: f32,
}

/// Drops rippling out across black water, shared by Raindrop and Drizzle. The pattern
/// decides when a drop lands; this draws it.
pub struct Ripples {
    rng: Rng,
    style: RippleStyle,
    drops: [Droplet; MAX_DROPS],
}

impl Ripples {
    pub fn new(seed: u32, style: RippleStyle) -> Self {
        Ripples {
            rng: Rng::new(seed),
            style,
            drops: core::array::from_fn(|_| Droplet {
                x: 0.0,
                y: 0.0,
                start_ms: 0,
                strength: 0.0,
                hue: HUE_MIN,
                spacing: 0.0,
                train: 0.0,
                alive: false,
            }),
        }
    }

    /// The generator drops are placed with, for patterns to pick their drop sizes from.
    pub fn rng(&mut self) -> &mut Rng {
        &mut self.rng
    }

    /// Where the next drop lands: at a random LED, so always on the fixture.
    pub fn spot(&mut self) -> Spot {
        let pick = self.rng.below(LED_COUNT);
        let (x, y) = (LED_MAP[pick].wx, LED_MAP[pick].wy);
        let hue = self.rng.range_f32(HUE_MIN, HUE_MAX);
        Spot { x, y, hue }
    }

    /// Lands a drop at `spot` with `count` rings `spacing` mm apart. With no slot free it
    /// replaces the dimmest drop.
    pub fn land(&mut self, spot: Spot, t_ms: u32, strength: f32, spacing: f32, count: f32) {
        let slot = self.drops.iter().position(|d| !d.alive).unwrap_or_else(|| {
            let glow = |i: &usize| self.drops[*i].glow(t_ms, &self.style);
            (0..MAX_DROPS)
                .min_by(|a, b| glow(a).total_cmp(&glow(b)))
                .unwrap_or(0)
        });
        self.drops[slot] = Droplet {
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

    pub fn draw(&mut self, t_ms: u32, out: &mut Frame) {
        let s = self.style;
        let blank = Ring {
            x: 0.0,
            y: 0.0,
            r: 0.0,
            u_inner: 0.0,
            u_outer: 0.0,
            amp: 0.0,
            hue: 0.0,
            sat_w: 0.0,
            inv_spacing: 0.0,
            inv_train: 0.0,
        };
        let mut live = [blank; MAX_DROPS];
        let mut n_live = 0;
        for drop in self.drops.iter_mut() {
            if !drop.alive {
                continue;
            }
            let (r, young, amp) = drop.front(t_ms, &s);
            if r - drop.train > s.ripple_max_mm {
                drop.alive = false;
                continue;
            }
            let inner = (r - drop.train).max(0.0);
            live[n_live] = Ring {
                x: drop.x,
                y: drop.y,
                r,
                u_inner: inner * inner,
                u_outer: r * r,
                amp,
                hue: drop.hue,
                sat_w: DROP_SAT * (1.0 - young),
                inv_spacing: 1.0 / drop.spacing,
                inv_train: 1.0 / drop.train,
            };
            n_live += 1;
        }

        let inv_reach = 1.0 / s.ripple_max_mm;

        for (o, led) in out.iter_mut().zip(LED_MAP.iter()) {
            let mut ripple = 0.0f32;
            let mut hue_acc = 0.0f32;
            let mut sat_acc = 0.0f32;
            for ring in live[..n_live].iter() {
                let dx = led.wx - ring.x;
                let dy = led.wy - ring.y;
                let u = dx * dx + dy * dy;
                if u > ring.u_outer || u < ring.u_inner {
                    continue;
                }
                let d = u.sqrt();
                // Distance behind the leading edge.
                let q = ring.r - d;
                // The 0.5 puts a crest on the leading edge.
                let cell = q * ring.inv_spacing + 0.5;
                // q is never negative here, so the cast is a floor.
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
                *o = hsv(
                    hue_acc * inv,
                    (sat_acc * inv).clamp(0.0, 1.0),
                    ripple.min(1.0),
                );
            } else {
                *o = [0, 0, 0];
            }
        }
    }
}
