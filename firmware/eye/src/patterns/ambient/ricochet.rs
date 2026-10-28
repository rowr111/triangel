use crate::audio::Audio;
use crate::led::grid;
use crate::led::map::{LED_COUNT, LED_MAP, WORLD_BOT, WORLD_CENTROID_Y, WORLD_CX, WORLD_TOP};
use crate::led::world::{WORLD_LEFT, WORLD_RIGHT};
use crate::patterns::{
    hsv, lerp, random_u32, time_step_ms, wrap360, Frame, HueMix, Pattern, Phasor, Rng,
};

// Comets and launching.
const MAX_COMETS: usize = 4;
const RESPAWN_MIN_MS: u32 = 500; // pause before a dead comet relaunches
const RESPAWN_MAX_MS: u32 = 2500;
const STAGGER_MS: u32 = 3000; // spread of the first launches on entry
const SPEED_MIN_MM_S: f32 = 50.0; // each comet picks a speed in this range
const SPEED_MAX_MM_S: f32 = 200.0;
const ENERGY_DECAY: f32 = 0.8; // energy kept per bounce
const ENERGY_MIN: f32 = 0.1; // below this the comet dies
const BOUNCE_TURN: f32 = 0.25; // random turn (rad) per bounce, so paths never loop
const MAX_BOUNCES: usize = 4; // walls one time step can bounce off
const CORNER_GAP: f32 = 0.12; // share of a wall kept clear of launches at each end
const ENTRY_SPREAD: f32 = 1.1; // most a launch can angle (rad) off straight in
const ENTRY_CLEAR_MM: f32 = 20.0; // a launch must cross its wall this far from the other two

// How a comet looks.
const HEAD_R_MIN_MM: f32 = 50.0; // radius of the fastest comet
const HEAD_R_MAX_MM: f32 = 95.0; // radius of the slowest comet
const HEAD_MIN_FRAC: f32 = 0.25; // head size at zero energy, relative to full
const EDGE_SHARP: f32 = 2.0; // above 1 sharpens the dot edge
const TRAIL_LEN: usize = 24; // trail points per comet
const TRAIL_R_MM: f32 = 18.0; // trail radius just behind the head
const TRAIL_TAPER: f32 = 0.8; // how much the trail narrows toward its tip
const BASE_SAT: f32 = 0.85;
const HEAD_WHITEN: f32 = 0.7; // how far bright cores go toward white

// Sparks thrown at each bounce.
const MAX_SPARKS: usize = 28; // sparks alive at once, shared by all comets
const SPARKS_PER_BOUNCE: f32 = 10.0; // at full energy
const SPARK_SPEED_FRAC: f32 = 1.5; // spark speed, relative to the comet's
const SPARK_SPEED_MIN: f32 = 0.7; // lowest random share of that speed
const SPARK_SPEED_RANGE: f32 = 0.6; // how much higher the share can go
const SPARK_DRAG: f32 = 0.0025; // fraction of velocity lost per ms
const SPARK_FAN: f32 = 1.5; // half-angle (rad) of the spray
const SPARK_LIFE_MS: f32 = 900.0;
const SPARK_R_MM: f32 = 15.0; // must exceed the LED spacing (about 10 mm)
const SPARK_BRIGHT: f32 = 2.5; // above 1 holds sparks at full brightness before fading
const SPARK_GRAVITY: f32 = 0.000_15; // downward pull (mm/ms^2)

const MAX_DT_MS: u32 = 60; // longest time step

// The three walls' inward unit normals. The left and right walls are measured from their
// top corners.
const TOP_NORMAL: (f32, f32) = (0.0, 1.0);
const LEFT_NORMAL: (f32, f32) = (0.866_025_4, -0.5);
const RIGHT_NORMAL: (f32, f32) = (-0.866_025_4, -0.5);

#[derive(Clone, Copy)]
struct Comet {
    alive: bool,
    entering: bool,  // still coming in from outside the wall, so no bouncing yet
    respawn_ms: u32, // when to launch, if not alive
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    energy: f32,
    hue: Phasor,
    radius: f32, // head radius at full energy
    speed: f32,  // mm/ms
    trail: [(f32, f32); TRAIL_LEN],
    trail_head: usize,
}

impl Comet {
    /// The head shrinks as the comet loses energy.
    fn head_radius(&self) -> f32 {
        self.radius * lerp(HEAD_MIN_FRAC, 1.0, self.energy)
    }
}

#[derive(Clone, Copy)]
struct Spark {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    life: f32,   // 1 at birth, 0 when dead
    hue: Phasor, // cos and sin of the hue angle
}

/// A round dot to draw: a comet head, a trail point or a spark.
struct Dot {
    x: f32,
    y: f32,
    inv_r2: f32,
    brightness: f32,
    hue: Phasor,
    radius: f32,
}

/// One LED's total brightness and hue vector this frame.
#[derive(Clone, Copy, Default)]
struct LedSum {
    brightness: f32,
    hue: HueMix,
}

/// Ricochet: a few comets bounce around inside the triangle on black, trailing tails. Each
/// bounce throws sparks and costs energy; a spent comet relaunches after a pause.
pub struct Ricochet {
    last_ms: u32,
    rng: Rng,
    comets: [Comet; MAX_COMETS],
    sparks: [Spark; MAX_SPARKS],
    dots: Vec<Dot>,
    // Per-LED brightness and hue-vector sums for this frame.
    sums: [LedSum; LED_COUNT],
}

impl Ricochet {
    pub fn new() -> Self {
        let comet = Comet {
            alive: false,
            entering: false,
            respawn_ms: 0,
            x: WORLD_CX,
            y: WORLD_CENTROID_Y,
            vx: 0.0,
            vy: 0.0,
            energy: 0.0,
            hue: Phasor { sin: 0.0, cos: 1.0 },
            radius: HEAD_R_MAX_MM,
            speed: 0.0,
            trail: [(WORLD_CX, WORLD_CENTROID_Y); TRAIL_LEN],
            trail_head: 0,
        };
        Ricochet {
            last_ms: 0,
            rng: Rng::new(1),
            comets: [comet; MAX_COMETS],
            sparks: [Spark {
                x: 0.0,
                y: 0.0,
                vx: 0.0,
                vy: 0.0,
                life: 0.0,
                hue: Phasor { sin: 0.0, cos: 1.0 },
            }; MAX_SPARKS],
            dots: Vec::with_capacity(MAX_COMETS * TRAIL_LEN + MAX_SPARKS),
            sums: [LedSum::default(); LED_COUNT],
        }
    }
}

impl Pattern for Ricochet {
    fn on_enter(&mut self, t_ms: u32) {
        // Reseed, clear the sparks, and stagger the first launches.
        self.last_ms = t_ms;
        self.rng = Rng::new(random_u32());
        for comet in self.comets.iter_mut() {
            comet.alive = false;
            let delay = self.rng.range_u32(0, STAGGER_MS);
            comet.respawn_ms = t_ms.wrapping_add(delay);
        }
        self.comets[0].respawn_ms = t_ms; // one comet right away
        for spark in self.sparks.iter_mut() {
            spark.life = 0.0;
        }
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_ms = time_step_ms(&mut self.last_ms, t_ms, MAX_DT_MS) as f32;

        for i in 0..MAX_COMETS {
            if self.comets[i].alive {
                self.step_comet(i, dt_ms, t_ms);
            } else if t_ms.wrapping_sub(self.comets[i].respawn_ms) as i32 >= 0 {
                self.launch(i);
            }
        }
        self.update_sparks(dt_ms);
        self.collect_dots();
        self.add_dots();

        for (i, px) in out.iter_mut().enumerate() {
            let sum = self.sums[i];
            let brightness = sum.brightness.min(1.0);
            if brightness <= 0.0 {
                *px = [0, 0, 0];
                continue;
            }
            let hue = sum.hue.degrees();
            let sat = BASE_SAT * (1.0 - brightness * brightness * HEAD_WHITEN);
            *px = hsv(wrap360(hue), sat, brightness);
        }
    }
}

impl Ricochet {
    /// Launches comet `i` inward from a random point on a random wall, in a new color.
    fn launch(&mut self, i: usize) {
        let wall = self.rng.below(3);
        let along = self.rng.range_f32(CORNER_GAP, 1.0 - CORNER_GAP); // stay off the corners
        let (wall_x, wall_y, normal) = match wall {
            0 => (lerp(WORLD_LEFT, WORLD_RIGHT, along), WORLD_TOP, TOP_NORMAL),
            1 => (
                lerp(WORLD_LEFT, WORLD_CX, along),
                lerp(WORLD_TOP, WORLD_BOT, along),
                LEFT_NORMAL,
            ),
            _ => (
                lerp(WORLD_RIGHT, WORLD_CX, along),
                lerp(WORLD_TOP, WORLD_BOT, along),
                RIGHT_NORMAL,
            ),
        };
        let base = normal.1.atan2(normal.0);
        let offset = self.rng.plus_minus(ENTRY_SPREAD);
        let hue = self.rng.f32() * 360.0;
        // Fast comets are small, slow ones big.
        let speed_frac = self.rng.f32();
        let speed = lerp(SPEED_MIN_MM_S, SPEED_MAX_MM_S, speed_frac) / 1000.0;
        let radius = lerp(HEAD_R_MAX_MM, HEAD_R_MIN_MM, speed_frac);
        // Start one radius outside the wall.
        let start_x = wall_x - normal.0 * radius;
        let start_y = wall_y - normal.1 * radius;
        // A path crossing its wall too near a corner never gets inside, so aim it the other way.
        let mut angle = base + offset;
        let reach = radius / offset.cos();
        let (top, left, right) =
            wall_dists(start_x + angle.cos() * reach, start_y + angle.sin() * reach);
        let clear = match wall {
            0 => left.min(right),
            1 => top.min(right),
            _ => top.min(left),
        };
        if clear < ENTRY_CLEAR_MM {
            angle = base - offset;
        }
        let comet = &mut self.comets[i];
        comet.alive = true;
        comet.entering = true;
        comet.radius = radius;
        comet.speed = speed;
        comet.x = start_x;
        comet.y = start_y;
        comet.vx = angle.cos() * speed;
        comet.vy = angle.sin() * speed;
        comet.energy = 1.0;
        comet.hue = Phasor::new(hue.to_radians());
        comet.trail_head = 0;
        for point in comet.trail.iter_mut() {
            *point = (start_x, start_y);
        }
    }

    /// Moves comet `i` by `dt_ms`, bouncing it off any wall it crosses.
    fn step_comet(&mut self, i: usize, dt_ms: f32, t_ms: u32) {
        let Ricochet {
            comets,
            sparks,
            rng,
            ..
        } = self;
        let comet = &mut comets[i];
        comet.x += comet.vx * dt_ms;
        comet.y += comet.vy * dt_ms;

        if comet.entering {
            let (top, left, right) = wall_dists(comet.x, comet.y);
            if top >= 0.0 && left >= 0.0 && right >= 0.0 {
                comet.entering = false;
            }
        } else {
            for _ in 0..MAX_BOUNCES {
                let (nearest, normal) = nearest_wall(comet.x, comet.y);
                if nearest >= 0.0 {
                    break;
                }

                // Move back to the wall and reflect the velocity.
                comet.x += -nearest * normal.0;
                comet.y += -nearest * normal.1;
                let inward_speed = comet.vx * normal.0 + comet.vy * normal.1;
                // Already moving inward, so this is rounding error, not a new bounce.
                if inward_speed >= 0.0 {
                    continue;
                }
                comet.vx -= 2.0 * inward_speed * normal.0;
                comet.vy -= 2.0 * inward_speed * normal.1;

                let turn = rng.plus_minus(BOUNCE_TURN);
                let (mut sin, cos) = turn.sin_cos();
                let (vx, vy) = (comet.vx, comet.vy);
                // Turn the other way if this turn would point back out through the wall.
                if (vx * cos - vy * sin) * normal.0 + (vx * sin + vy * cos) * normal.1 <= 0.0 {
                    sin = -sin;
                }
                comet.vx = vx * cos - vy * sin;
                comet.vy = vx * sin + vy * cos;

                let count = (SPARKS_PER_BOUNCE * comet.energy) as usize;
                spawn_sparks(sparks, rng, comet, count, normal);
                comet.energy *= ENERGY_DECAY;
            }
        }

        comet.trail_head = (comet.trail_head + 1) % TRAIL_LEN;
        comet.trail[comet.trail_head] = (comet.x, comet.y);

        if comet.energy < ENERGY_MIN {
            comet.alive = false;
            let delay = rng.range_u32(RESPAWN_MIN_MS, RESPAWN_MAX_MS);
            comet.respawn_ms = t_ms.wrapping_add(delay);
        }
    }

    /// Moves the live sparks with gravity and drag, and ages them.
    fn update_sparks(&mut self, dt_ms: f32) {
        let drag = (1.0 - SPARK_DRAG * dt_ms).max(0.0);
        for spark in self.sparks.iter_mut() {
            if spark.life > 0.0 {
                spark.vy += SPARK_GRAVITY * dt_ms;
                spark.vx *= drag;
                spark.vy *= drag;
                spark.x += spark.vx * dt_ms;
                spark.y += spark.vy * dt_ms;
                spark.life -= dt_ms / SPARK_LIFE_MS;
            }
        }
    }

    /// This frame's dots: each comet's head and trail, then the live sparks.
    fn collect_dots(&mut self) {
        self.dots.clear();
        for comet in self.comets.iter().filter(|c| c.alive) {
            let hue = comet.hue;
            let energy = comet.energy;
            let head = comet.trail_head;
            for back in 0..TRAIL_LEN {
                let idx = (head + TRAIL_LEN - back) % TRAIL_LEN; // back = 0 is the head
                let (x, y) = comet.trail[idx];
                let along = back as f32 / TRAIL_LEN as f32;
                let (radius, brightness) = if back == 0 {
                    (comet.head_radius(), energy)
                } else {
                    (
                        TRAIL_R_MM * (1.0 - TRAIL_TAPER * along),
                        (1.0 - along) * energy,
                    )
                };
                self.dots.push(Dot {
                    x,
                    y,
                    inv_r2: 1.0 / (radius * radius),
                    brightness,
                    hue,
                    radius,
                });
            }
        }
        for spark in &self.sparks {
            if spark.life > 0.0 {
                self.dots.push(Dot {
                    x: spark.x,
                    y: spark.y,
                    inv_r2: 1.0 / (SPARK_R_MM * SPARK_R_MM),
                    brightness: (spark.life * SPARK_BRIGHT).min(1.0),
                    hue: spark.hue,
                    radius: SPARK_R_MM,
                });
            }
        }
    }

    /// Adds each dot to the LEDs it covers.
    fn add_dots(&mut self) {
        self.sums.fill(LedSum::default());
        for dot in &self.dots {
            grid::for_each_near(dot.x, dot.y, dot.radius, |i| {
                let led = &LED_MAP[i];
                let dx = led.wx - dot.x;
                let dy = led.wy - dot.y;
                let strength = ((1.0 - (dx * dx + dy * dy) * dot.inv_r2) * EDGE_SHARP).min(1.0);
                if strength > 0.0 {
                    let weight = strength * dot.brightness;
                    let sum = &mut self.sums[i];
                    sum.brightness += weight;
                    sum.hue.add(weight, dot.hue);
                }
            });
        }
    }
}

/// Signed distance from (x, y) to the top, left and right walls; negative is outside.
fn wall_dists(x: f32, y: f32) -> (f32, f32, f32) {
    (
        y - WORLD_TOP,
        (x - WORLD_LEFT) * LEFT_NORMAL.0 + (y - WORLD_TOP) * LEFT_NORMAL.1,
        (x - WORLD_RIGHT) * RIGHT_NORMAL.0 + (y - WORLD_TOP) * RIGHT_NORMAL.1,
    )
}

/// The nearest wall's signed distance from (x, y), and its inward normal.
fn nearest_wall(x: f32, y: f32) -> (f32, (f32, f32)) {
    let (top, left, right) = wall_dists(x, y);
    let (mut nearest, mut normal) = (top, TOP_NORMAL);
    if left < nearest {
        nearest = left;
        normal = LEFT_NORMAL;
    }
    if right < nearest {
        nearest = right;
        normal = RIGHT_NORMAL;
    }
    (nearest, normal)
}

/// Sprays up to `count` sparks from `comet`, fanned around the wall's inward `normal`.
fn spawn_sparks(
    sparks: &mut [Spark; MAX_SPARKS],
    rng: &mut Rng,
    comet: &Comet,
    count: usize,
    normal: (f32, f32),
) {
    let radius = comet.head_radius();
    let base = normal.1.atan2(normal.0);
    let mut spawned = 0;
    for spark in sparks.iter_mut() {
        if spawned >= count {
            break;
        }
        if spark.life <= 0.0 {
            // Start on the comet's edge, so the spark is not hidden inside the bright head.
            let angle = base + rng.plus_minus(SPARK_FAN);
            let (dx, dy) = (angle.cos(), angle.sin());
            let speed =
                comet.speed * SPARK_SPEED_FRAC * (SPARK_SPEED_MIN + rng.f32() * SPARK_SPEED_RANGE);
            *spark = Spark {
                x: comet.x + dx * radius,
                y: comet.y + dy * radius,
                vx: dx * speed,
                vy: dy * speed,
                life: 1.0,
                hue: comet.hue,
            };
            spawned += 1;
        }
    }
}
