use crate::led::map::{Led, LED_COUNT, LED_MAP};
use crate::patterns::{hsv, Frame, Rng};

/// Enough that a drop is rarely replaced while its ripple is still crossing the fixture.
const MAX_DROPS: usize = 18;

/// Hue range for drops. Narrow, so overlapping rings can average their hues directly.
const HUE_MIN: f32 = 170.0;
const HUE_MAX: f32 = 250.0;
/// Below 1 because a fully saturated blue lights only the blue channel and looks dim.
const DROP_SAT: f32 = 0.62;
/// An LED whose rings add up to less than this is black.
const MIN_RIPPLE: f32 = 1e-3;

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
    fn front(&self, t_ms: u32, style: &RippleStyle) -> (f32, f32, f32) {
        let radius =
            style.start_radius_mm + t_ms.wrapping_sub(self.start_ms) as f32 * style.ripple_speed;
        let young = (1.0 - (radius - style.start_radius_mm) / style.white_mm).clamp(0.0, 1.0);
        let brightness = self.strength
            * (style.gain_base + style.gain_hit * self.strength)
            * (1.0 + style.hit_boost * young);
        (radius, young, brightness)
    }

    /// Roughly how bright the brightest point of the rings is at `t_ms`.
    fn glow(&self, t_ms: u32, style: &RippleStyle) -> f32 {
        let (radius, _, brightness) = self.front(t_ms, style);
        let inner = (radius - self.train).max(0.0);
        let peak_dist = ((style.ripple_max_mm + radius - self.train) * 0.5)
            .max(inner)
            .min(radius.min(style.ripple_max_mm));
        brightness
            * (1.0 - (radius - peak_dist) / self.train)
            * (1.0 - peak_dist / style.ripple_max_mm)
    }
}

/// One live drop's geometry for this frame.
struct Ring {
    x: f32,
    y: f32,
    radius: f32,
    inner_d2: f32,
    outer_d2: f32,
    /// Includes the drop's gain and the boost while it is young.
    brightness: f32,
    hue: f32,
    /// Saturation: 0 while the ripple is still white, rising to DROP_SAT.
    sat: f32,
    inv_spacing: f32,
    inv_train: f32,
}

/// Drops rippling out across black water, shared by Raindrop and Drizzle. The pattern
/// decides when a drop lands; this draws it.
pub struct Ripples {
    rng: Rng,
    style: RippleStyle,
    drops: [Droplet; MAX_DROPS],
    rings: Vec<Ring>,
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
            rings: Vec::with_capacity(MAX_DROPS),
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
        self.collect_rings(t_ms);
        let inv_reach = 1.0 / self.style.ripple_max_mm;
        for (px, led) in out.iter_mut().zip(LED_MAP.iter()) {
            *px = self.color_at(led, inv_reach);
        }
    }

    /// Ages the drops and lists the live ones' rings for this frame.
    fn collect_rings(&mut self, t_ms: u32) {
        let style = self.style;
        self.rings.clear();
        for drop in self.drops.iter_mut() {
            if !drop.alive {
                continue;
            }
            let (radius, young, brightness) = drop.front(t_ms, &style);
            if radius - drop.train > style.ripple_max_mm {
                drop.alive = false;
                continue;
            }
            let inner = (radius - drop.train).max(0.0);
            self.rings.push(Ring {
                x: drop.x,
                y: drop.y,
                radius,
                inner_d2: inner * inner,
                outer_d2: radius * radius,
                brightness,
                hue: drop.hue,
                sat: DROP_SAT * (1.0 - young),
                inv_spacing: 1.0 / drop.spacing,
                inv_train: 1.0 / drop.train,
            });
        }
    }

    /// The color the rings give one LED.
    fn color_at(&self, led: &Led, inv_reach: f32) -> [u8; 3] {
        let mut ripple = 0.0f32;
        let mut hue_sum = 0.0f32;
        let mut sat_sum = 0.0f32;
        for ring in &self.rings {
            let dx = led.wx - ring.x;
            let dy = led.wy - ring.y;
            let d2 = dx * dx + dy * dy;
            if d2 > ring.outer_d2 || d2 < ring.inner_d2 {
                continue;
            }
            let dist = d2.sqrt();
            // Distance behind the leading edge.
            let behind = ring.radius - dist;
            // The 0.5 puts a crest on the leading edge.
            let cell = behind * ring.inv_spacing + 0.5;
            // behind is never negative here, so the cast is a floor.
            let frac = cell - (cell as u32) as f32;
            // Triangle wave, squared for a sharper crest.
            let crest = 1.0 - (2.0 * frac - 1.0).abs();
            let brightness = ring.brightness
                * crest
                * crest
                * (1.0 - behind * ring.inv_train)
                * (1.0 - dist * inv_reach).max(0.0);
            ripple += brightness;
            hue_sum += ring.hue * brightness;
            sat_sum += ring.sat * brightness;
        }

        if ripple > MIN_RIPPLE {
            let inv = 1.0 / ripple;
            hsv(
                hue_sum * inv,
                (sat_sum * inv).clamp(0.0, 1.0),
                ripple.min(1.0),
            )
        } else {
            [0, 0, 0]
        }
    }
}
