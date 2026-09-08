use core::f32::consts::TAU;

use crate::audio::Audio;
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, ReactivePattern, hsv};

const MAX_SPARKS: usize = 56;
/// Sparks a burst throws: every hit gets BURST_MIN, a full one adds BURST_EXTRA.
const BURST_MIN: usize = 8;
const BURST_EXTRA: f32 = 8.0;

/// Spark speed in mm per ms, and how long one lasts.
const SPEED_MIN: f32 = 0.08;
const SPEED_MAX: f32 = 0.20;
const SPARK_LIFE_MS: f32 = 1500.0;
/// Downward pull in mm per ms squared, so the shower droops as it falls.
const GRAVITY: f32 = 0.00005;
/// Spark size at the burst and at the end of its life.
const SPARK_START_MM: f32 = 30.0;
const SPARK_END_MM: f32 = 12.0;
/// Fraction of its life a spark spends turning from white to its color.
const WHITE_FRAC: f32 = 0.22;
/// Extra brightness over that same moment.
const HIT_BOOST: f32 = 2.2;

/// A white churn under everything, on its own slow clock rather than the music, so the
/// fixture is never empty between bursts. One sine per axis, each with its phase pushed
/// around by a sine of the other axis at an off-integer ratio, so the two never line up
/// and it turns over rather than sweeping across.
const WASH_BASE: f32 = 0.30;
const WASH_DEPTH: f32 = 0.20;
const WASH_CELL_MM: f32 = 180.0;
const WASH_PERIOD_MS: u32 = 9_000;
const WASH2_PERIOD_MS: u32 = 6_100;
const WASH_CROSS_Y: f32 = 0.83;
const WASH_CROSS_X: f32 = 0.71;

/// Firework colors. Saturation runs high for the warm hues where the LEDs can carry it,
/// and is eased back toward blue and violet, which otherwise drive one channel and read
/// dim rather than vivid.
const PALETTE: [(f32, f32); 5] =
    [(45.0, 0.95), (12.0, 0.97), (330.0, 0.90), (190.0, 0.82), (130.0, 0.88)];

/// No spark owns this LED, so it keeps the background's white.
const NO_OWNER: u8 = u8::MAX;

struct Spark {
    x:        f32,
    y:        f32,
    vx:       f32,
    vy:       f32,
    hue:      f32,
    sat:      f32,
    start_ms: u32,
    strength: f32,
    alive:    bool,
}

/// One live spark reduced to what the splat needs.
#[derive(Clone, Copy)]
struct Live {
    px:     f32,
    py:     f32,
    r2:     f32,
    inv_r2: f32,
    amp:    f32,
    hue:    f32,
    sat:    f32,
    reach:  usize,
}

/// Fireworks. Each beat bursts at one point and throws a shower of sparks outward,
/// white at the burst, settling into one color as they fly, drooping and fading out.
pub struct Firework {
    rng:    u32,
    sparks: [Spark; MAX_SPARKS],
    /// sin and cos of each axis' modulating term, which have no time in them and so are
    /// computed once.
    wy_sin: [f32; LED_COUNT],
    wy_cos: [f32; LED_COUNT],
    wx_sin: [f32; LED_COUNT],
    wx_cos: [f32; LED_COUNT],
    /// Per-LED accumulation, so each spark touches only the LEDs near it rather than
    /// every LED testing itself against every spark.
    tot:   [f32; LED_COUNT],
    best:  [f32; LED_COUNT],
    owner: [u8; LED_COUNT],
}

impl Firework {
    pub fn new(leds: &[Led]) -> Self {
        let k = TAU / WASH_CELL_MM;
        Firework {
            rng:    0x1234_5678,
            sparks: core::array::from_fn(|_| Spark {
                x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, hue: 0.0, sat: 0.0,
                start_ms: 0, strength: 0.0, alive: false,
            }),
            wy_sin: core::array::from_fn(|i| (leds[i].wy * k * WASH_CROSS_Y).sin()),
            wy_cos: core::array::from_fn(|i| (leds[i].wy * k * WASH_CROSS_Y).cos()),
            wx_sin: core::array::from_fn(|i| (leds[i].wx * k * WASH_CROSS_X).sin()),
            wx_cos: core::array::from_fn(|i| (leds[i].wx * k * WASH_CROSS_X).cos()),
            tot:   [0.0; LED_COUNT],
            best:  [0.0; LED_COUNT],
            owner: [NO_OWNER; LED_COUNT],
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

    fn free_slot(&self) -> usize {
        self.sparks.iter().position(|s| !s.alive).unwrap_or_else(|| {
            let mut oldest = 0;
            for (i, s) in self.sparks.iter().enumerate() {
                if s.start_ms < self.sparks[oldest].start_ms {
                    oldest = i;
                }
            }
            oldest
        })
    }

    /// Burst at a random LED, throwing `count` sparks outward in one color.
    fn burst(&mut self, leds: &[Led], t_ms: u32, strength: f32, count: usize) {
        let pick = (self.randf() * LED_COUNT as f32) as usize % LED_COUNT;
        let (x, y) = (leds[pick].wx, leds[pick].wy);
        let (hue, sat) = PALETTE[(self.randf() * PALETTE.len() as f32) as usize % PALETTE.len()];
        // Spread the sparks around the circle rather than leaving them to clump.
        let step = TAU / count as f32;
        let offset = self.randf() * TAU;
        for k in 0..count {
            let angle = offset + step * k as f32 + (self.randf() - 0.5) * step;
            let speed = SPEED_MIN + self.randf() * (SPEED_MAX - SPEED_MIN);
            let (sn, cs) = angle.sin_cos();
            let slot = self.free_slot();
            self.sparks[slot] = Spark {
                x,
                y,
                vx: cs * speed,
                vy: sn * speed,
                hue,
                sat,
                start_ms: t_ms,
                strength,
                alive: true,
            };
        }
    }
}

impl ReactivePattern for Firework {
    fn render(&mut self, leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.beat {
            let count = BURST_MIN + (audio.beat_strength * BURST_EXTRA) as usize;
            self.burst(leds, t_ms, 0.4 + 0.6 * audio.beat_strength, count);
        }

        // Age the sparks and gather the live ones, retiring any that are spent.
        let blank = Live {
            px: 0.0, py: 0.0, r2: 0.0, inv_r2: 0.0, amp: 0.0, hue: 0.0, sat: 0.0, reach: 0,
        };
        let mut live = [blank; MAX_SPARKS];
        let mut n_live = 0;
        for spark in self.sparks.iter_mut() {
            if !spark.alive {
                continue;
            }
            let age = t_ms.wrapping_sub(spark.start_ms) as f32;
            if age >= SPARK_LIFE_MS {
                spark.alive = false;
                continue;
            }
            let life = 1.0 - age / SPARK_LIFE_MS;
            let young = (1.0 - age / (SPARK_LIFE_MS * WHITE_FRAC)).clamp(0.0, 1.0);
            let r = SPARK_END_MM + (SPARK_START_MM - SPARK_END_MM) * life;
            live[n_live] = Live {
                px: spark.x + spark.vx * age,
                py: spark.y + spark.vy * age + GRAVITY * age * age,
                r2: r * r,
                inv_r2: 1.0 / (r * r),
                amp: spark.strength * life * life * (1.0 + HIT_BOOST * young),
                hue: spark.hue,
                sat: spark.sat * (1.0 - young),
                reach: (r / CELL_MM).ceil() as usize,
            };
            n_live += 1;
        }

        let wash1 = (t_ms % WASH_PERIOD_MS) as f32 / WASH_PERIOD_MS as f32 * TAU;
        let wash2 = (t_ms % WASH2_PERIOD_MS) as f32 / WASH2_PERIOD_MS as f32 * TAU;
        let (w1_sin, w1_cos) = wash1.sin_cos();
        let (w2_sin, w2_cos) = wash2.sin_cos();
        let wash_k = TAU / WASH_CELL_MM;

        let Firework { wy_sin, wy_cos, wx_sin, wx_cos, tot, best, owner, .. } = self;

        // Background into the accumulator. The outer sines cannot be factored out, since
        // their arguments contain the modulating terms.
        for (i, led) in leds.iter().enumerate() {
            let mod_y = wy_sin[i] * w2_cos + wy_cos[i] * w2_sin;
            let mod_x = wx_sin[i] * w1_cos + wx_cos[i] * w1_sin;
            let churn = ((led.wx * wash_k + mod_y + wash1).sin()
                + (led.wy * wash_k + mod_x + wash2).sin())
                * 0.5;
            tot[i] = WASH_BASE + WASH_DEPTH * churn;
            best[i] = 0.0;
            owner[i] = NO_OWNER;
        }

        // Each spark touches only the LEDs in the grid cells it covers. The strongest
        // spark over an LED keeps its color, so overlapping bursts stay distinct.
        for (s, spark) in live[..n_live].iter().enumerate() {
            grid::for_each_near(spark.px, spark.py, spark.reach, |i| {
                let dx = leds[i].wx - spark.px;
                let dy = leds[i].wy - spark.py;
                let u = dx * dx + dy * dy;
                if u > spark.r2 {
                    return;
                }
                let fall = 1.0 - u * spark.inv_r2;
                let w = spark.amp * fall * fall;
                tot[i] += w;
                if w > best[i] {
                    best[i] = w;
                    owner[i] = s as u8;
                }
            });
        }

        for (i, o) in out.iter_mut().enumerate() {
            let (hue, sat) = match owner[i] {
                NO_OWNER => (0.0, 0.0),
                s => (live[s as usize].hue, live[s as usize].sat),
            };
            *o = hsv(hue, sat, tot[i].min(1.0));
        }
    }
}
