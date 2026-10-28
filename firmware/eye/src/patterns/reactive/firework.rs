use core::f32::consts::TAU;

use crate::audio::Audio;
use crate::led::grid;
use crate::led::map::{LED_COUNT, LED_MAP};
use crate::patterns::{free_or_oldest, hsv, lerp, Frame, Pattern, Phasor, Rng};
use triangel_shared::tuning::firework::*;

/// Must stay under 255: each LED stores the index of its strongest spark in a byte.
const MAX_SPARKS: usize = 192;
/// No spark reaches this LED.
const NO_OWNER: u8 = u8::MAX;
const _: () = assert!(
    MAX_SPARKS <= NO_OWNER as usize,
    "a spark index must fit below NO_OWNER"
);
const _: () = assert!(
    DROP_BURSTS * (BURST_MIN + BURST_EXTRA as usize) <= MAX_SPARKS,
    "a drop's bursts need more sparks than MAX_SPARKS"
);
/// Spots tried for each drop burst before taking the farthest one found.
const DROP_TRIES: usize = 12;
/// A burst's strength on the softest beat; the hardest beat gives 1.
const SOFTEST_BURST: f32 = 0.4;

/// (hue, sat). The cooler hues are less saturated, or they would look dim.
const PALETTE: [(f32, f32); 5] = [
    (45.0, 0.95),
    (12.0, 0.97),
    (330.0, 0.90),
    (190.0, 0.82),
    (130.0, 0.88),
];

struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    hue: f32,
    sat: f32,
    start_ms: u32,
    strength: f32,
    alive: bool,
}

/// One live spark's position, size and color for this frame.
struct LiveSpark {
    x: f32,
    y: f32,
    r2: f32,
    inv_r2: f32,
    brightness: f32,
    hue: f32,
    sat: f32,
    radius: f32,
}

/// One LED's total brightness, its strongest spark's brightness, and that spark's index.
#[derive(Clone, Copy)]
struct LedSum {
    total: f32,
    best: f32,
    owner: u8,
}

impl LedSum {
    const NONE: LedSum = LedSum {
        total: 0.0,
        best: 0.0,
        owner: NO_OWNER,
    };
}

/// Fireworks. Each beat throws a burst of sparks from one point; they start white, take
/// on the burst's color, fall and fade.
pub struct Firework {
    rng: Rng,
    sparks: [Spark; MAX_SPARKS],
    live: Vec<LiveSpark>,
    /// Per LED: total brightness, the strongest spark's brightness, and its index.
    sums: [LedSum; LED_COUNT],
}

impl Firework {
    pub fn new() -> Self {
        Firework {
            rng: Rng::new(0x1234_5678),
            sparks: core::array::from_fn(|_| Spark {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                hue: 0.0,
                sat: 0.0,
                start_ms: 0,
                strength: 0.0,
                alive: false,
            }),
            live: Vec::with_capacity(MAX_SPARKS),
            sums: [LedSum::NONE; LED_COUNT],
        }
    }
}

impl Pattern for Firework {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            self.drop_bursts(t_ms);
        } else if audio.beat {
            let count = BURST_MIN + (audio.beat_strength * BURST_EXTRA) as usize;
            let strength = lerp(SOFTEST_BURST, 1.0, audio.beat_strength);
            self.burst(t_ms, strength, count);
        }

        self.collect_live(t_ms);
        self.add_sparks();

        for (i, px) in out.iter_mut().enumerate() {
            let sum = self.sums[i];
            let (hue, sat) = match sum.owner {
                NO_OWNER => (0.0, 0.0),
                owner => (self.live[owner as usize].hue, self.live[owner as usize].sat),
            };
            *px = hsv(hue, sat, sum.total.min(1.0));
        }
    }
}

impl Firework {
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
        let min_d2 = DROP_SPACING_MM * DROP_SPACING_MM;
        let mut placed = [(0.0f32, 0.0f32); DROP_BURSTS];
        for k in 0..DROP_BURSTS {
            let mut best = self.random_point();
            let mut best_d2 = nearest_d2(best, &placed[..k]);
            for _ in 0..DROP_TRIES {
                if best_d2 >= min_d2 {
                    break;
                }
                let spot = self.random_point();
                let d2 = nearest_d2(spot, &placed[..k]);
                if d2 > best_d2 {
                    best = spot;
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
            let angle = offset + step * k as f32 + self.rng.plus_minus(step * 0.5);
            let speed = self.rng.range_f32(SPEED_MIN, SPEED_MAX);
            let dir = Phasor::new(angle);
            let slot = free_or_oldest(&self.sparks, t_ms, |s| !s.alive, |s| s.start_ms);
            self.sparks[slot] = Spark {
                x,
                y,
                vx: dir.cos * speed,
                vy: dir.sin * speed,
                hue,
                sat,
                start_ms: t_ms,
                strength,
                alive: true,
            };
        }
    }

    /// Ages the sparks and lists the live ones' position, size and color for this frame.
    fn collect_live(&mut self, t_ms: u32) {
        self.live.clear();
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
            let radius = lerp(SPARK_END_MM, SPARK_START_MM, life);
            self.live.push(LiveSpark {
                x: spark.x + spark.vx * age,
                y: spark.y + spark.vy * age + GRAVITY * age * age,
                r2: radius * radius,
                inv_r2: 1.0 / (radius * radius),
                brightness: spark.strength * life * life * (1.0 + HIT_BOOST * young),
                hue: spark.hue,
                sat: spark.sat * (1.0 - young),
                radius,
            });
        }
    }

    /// Adds the live sparks to the LEDs they cover.
    fn add_sparks(&mut self) {
        self.sums.fill(LedSum::NONE);
        // An LED takes the color of its strongest spark, so overlapping bursts stay distinct.
        for (index, spark) in self.live.iter().enumerate() {
            grid::for_each_near(spark.x, spark.y, spark.radius, |i| {
                let dx = LED_MAP[i].wx - spark.x;
                let dy = LED_MAP[i].wy - spark.y;
                let d2 = dx * dx + dy * dy;
                if d2 > spark.r2 {
                    return;
                }
                let fall = 1.0 - d2 * spark.inv_r2;
                let brightness = spark.brightness * fall * fall;
                let sum = &mut self.sums[i];
                sum.total += brightness;
                if brightness > sum.best {
                    sum.best = brightness;
                    sum.owner = index as u8;
                }
            });
        }
    }
}

/// Squared distance from `p` to the nearest of `others`, or f32::MAX if there are none.
fn nearest_d2(p: (f32, f32), others: &[(f32, f32)]) -> f32 {
    others.iter().fold(f32::MAX, |nearest, q| {
        let (dx, dy) = (p.0 - q.0, p.1 - q.1);
        nearest.min(dx * dx + dy * dy)
    })
}
