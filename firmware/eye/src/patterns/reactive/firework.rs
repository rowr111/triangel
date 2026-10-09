use core::f32::consts::TAU;

use crate::audio::Audio;
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{LED_COUNT, LED_MAP};
use crate::patterns::{Frame, Pattern, Rng, free_or_oldest, hsv};
use triangel_shared::tuning::firework::*;

/// Must stay under 255: each LED stores the index of its strongest spark in a byte.
const MAX_SPARKS: usize = 192;
/// Spots tried for each drop burst before taking the farthest one found.
const DROP_TRIES: usize = 12;

/// (hue, sat). The cooler hues are less saturated, or they would look dim.
const PALETTE: [(f32, f32); 5] =
    [(45.0, 0.95), (12.0, 0.97), (330.0, 0.90), (190.0, 0.82), (130.0, 0.88)];

/// Squared distance from `p` to the nearest of `others`, or f32::MAX if there are none.
fn nearest_d2(p: (f32, f32), others: &[(f32, f32)]) -> f32 {
    others.iter().fold(f32::MAX, |m, q| {
        let (dx, dy) = (p.0 - q.0, p.1 - q.1);
        m.min(dx * dx + dy * dy)
    })
}

/// No spark reaches this LED.
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

/// One live spark's position, size and color for this frame.
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

/// Fireworks. Each beat throws a burst of sparks from one point; they start white, take
/// on the burst's color, fall and fade.
pub struct Firework {
    rng:    Rng,
    sparks: [Spark; MAX_SPARKS],
    /// Per LED: total brightness, the strongest spark's brightness, and its index.
    tot:   [f32; LED_COUNT],
    best:  [f32; LED_COUNT],
    owner: [u8; LED_COUNT],
}

impl Firework {
    pub fn new() -> Self {
        Firework {
            rng:    Rng::new(0x1234_5678),
            sparks: core::array::from_fn(|_| Spark {
                x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, hue: 0.0, sat: 0.0,
                start_ms: 0, strength: 0.0, alive: false,
            }),
            tot:   [0.0; LED_COUNT],
            best:  [0.0; LED_COUNT],
            owner: [NO_OWNER; LED_COUNT],
        }
    }

    /// A random LED's position.
    fn random_point(&mut self) -> (f32, f32) {
        let pick = self.rng.below(LED_COUNT);
        (LED_MAP[pick].wx, LED_MAP[pick].wy)
    }

    fn burst(&mut self, t_ms: u32, strength: f32, count: usize) {
        let (x, y) = self.random_point();
        self.burst_at(x, y, t_ms, strength, count);
    }

    /// A drop: many full bursts at once, kept DROP_SPACING_MM apart where possible.
    fn drop_bursts(&mut self, t_ms: u32) {
        let count = BURST_MIN + BURST_EXTRA as usize;
        let min2 = DROP_SPACING_MM * DROP_SPACING_MM;
        let mut placed = [(0.0f32, 0.0f32); DROP_BURSTS];
        for k in 0..DROP_BURSTS {
            let mut best = self.random_point();
            let mut best_d2 = nearest_d2(best, &placed[..k]);
            for _ in 0..DROP_TRIES {
                if best_d2 >= min2 {
                    break;
                }
                let p = self.random_point();
                let d2 = nearest_d2(p, &placed[..k]);
                if d2 > best_d2 {
                    best = p;
                    best_d2 = d2;
                }
            }
            placed[k] = best;
            self.burst_at(best.0, best.1, t_ms, 1.0, count);
        }
    }

    /// Throws `count` sparks outward from (x, y) in one color.
    fn burst_at(&mut self, x: f32, y: f32, t_ms: u32, strength: f32, count: usize) {
        let (hue, sat) = self.rng.pick(&PALETTE);
        // One spark per equal slice of the circle, so they do not clump.
        let step = TAU / count as f32;
        let offset = self.rng.f32() * TAU;
        for k in 0..count {
            let angle = offset + step * k as f32 + (self.rng.f32() - 0.5) * step;
            let speed = SPEED_MIN + self.rng.f32() * (SPEED_MAX - SPEED_MIN);
            let (sn, cs) = angle.sin_cos();
            let slot = free_or_oldest(&self.sparks, t_ms, |s| !s.alive, |s| s.start_ms);
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

impl Pattern for Firework {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            self.drop_bursts(t_ms);
        } else if audio.beat {
            let count = BURST_MIN + (audio.beat_strength * BURST_EXTRA) as usize;
            self.burst(t_ms, 0.4 + 0.6 * audio.beat_strength, count);
        }

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

        let Firework { tot, best, owner, .. } = self;
        tot.fill(0.0);
        best.fill(0.0);
        owner.fill(NO_OWNER);

        // An LED takes the color of its strongest spark, so overlapping bursts stay distinct.
        for (s, spark) in live[..n_live].iter().enumerate() {
            grid::for_each_near(spark.px, spark.py, spark.reach, |i| {
                let dx = LED_MAP[i].wx - spark.px;
                let dy = LED_MAP[i].wy - spark.py;
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
