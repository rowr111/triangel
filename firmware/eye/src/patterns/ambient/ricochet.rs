use crate::audio::Audio;
use crate::patterns::{Frame, Pattern, Rng, hsv, lerp, wrap360};
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{WORLD_TOP, WORLD_BOT, WORLD_CX, WORLD_CENTROID_Y, LED_COUNT, LED_MAP};
use crate::led::world::{WORLD_LEFT, WORLD_RIGHT};

// Ricochet: a few comets bounce around inside the triangle on black, trailing tails. Each
// bounce throws sparks and costs energy; a spent comet relaunches after a pause.

const MAX_COMETS:     usize = 4;
const RESPAWN_MIN_MS: u32 = 500;    // pause before a dead comet relaunches
const RESPAWN_MAX_MS: u32 = 2500;
const STAGGER_MS:     f32 = 3000.0; // spread of the first launches on entry

const SPEED_MIN_MM_S: f32 = 50.0;  // each comet picks a speed in this range
const SPEED_MAX_MM_S: f32 = 200.0;
const ENERGY_DECAY:   f32 = 0.8;   // energy kept per bounce
const ENERGY_MIN:     f32 = 0.1;   // below this the comet dies
const BOUNCE_PERTURB: f32 = 0.15;  // random turn (rad) per bounce, so paths never loop
const ENTRY_SPREAD:   f32 = 0.9;   // most a launch can angle (rad) off straight in

const HEAD_R_MIN_MM: f32 = 50.0;  // radius of the fastest comet
const HEAD_R_MAX_MM: f32 = 95.0;  // radius of the slowest comet
const EDGE_SHARP:  f32   = 2.0;   // above 1 sharpens the dot edge
const TRAIL_LEN:   usize = 24;    // trail points per comet
const TRAIL_R_MM:  f32   = 18.0;  // tail radius just behind the head
const TAIL_TAPER:  f32   = 0.8;   // how much the tail narrows toward its tip
const BASE_SAT:    f32   = 0.85;
const HEAD_WHITEN: f32   = 0.7;   // how far bright cores go toward white

const SPARK_POOL:        usize = 28;    // sparks alive at once, shared by all comets
const SPARKS_PER_BOUNCE: f32   = 10.0;  // at full energy
const SPARK_SPEED_FRAC:  f32   = 1.5;   // spark speed, relative to the comet's
const SPARK_DRAG:        f32   = 0.0025;// fraction of velocity lost per ms
const SPARK_FAN:         f32   = 1.5;   // half-angle (rad) of the spray
const SPARK_LIFE_MS:     f32   = 900.0;
const SPARK_R_MM:        f32   = 15.0;  // must exceed the LED spacing (about 10 mm)
const SPARK_BRIGHT:      f32   = 2.5;   // above 1 holds sparks at full brightness before fading
const SPARK_GRAVITY:     f32   = 0.000_15; // downward pull (mm/ms^2)

const MAX_DT_MS: f32 = 60.0; // longest time step

// The three walls' inward unit normals. The left and right walls are measured from their
// top corners.
const N_TOP:   (f32, f32) = (0.0, 1.0);
const N_LEFT:  (f32, f32) = (0.866_025_4, -0.5);
const N_RIGHT: (f32, f32) = (-0.866_025_4, -0.5);

#[derive(Clone, Copy)]
struct Comet {
    alive:      bool,
    entering:   bool, // still coming in from outside the wall, so no bouncing yet
    respawn_at: u32, // when to launch, if not alive
    x:      f32,
    y:      f32,
    vx:     f32,
    vy:     f32,
    energy: f32,
    hue:    f32,
    radius: f32, // head radius at full energy
    speed:  f32, // mm/ms
    trail:      [(f32, f32); TRAIL_LEN],
    trail_head: usize,
}

#[derive(Clone, Copy)]
struct Spark {
    x:    f32,
    y:    f32,
    vx:   f32,
    vy:   f32,
    life: f32,  // 1 at birth, 0 when dead
    chue: f32,  // cos and sin of the hue angle
    shue: f32,
}

// A round dot to draw: a comet head, a trail point or a spark.
#[derive(Clone, Copy)]
struct Dot {
    x:      f32,
    y:      f32,
    inv_r2: f32,
    w:      f32, // brightness
    chue:   f32,
    shue:   f32,
    reach:  usize, // radius in grid cells
}

pub struct Ricochet {
    prev_ms: u32,
    rng:     Rng,
    comets:  [Comet; MAX_COMETS],
    sparks:  [Spark; SPARK_POOL],
    // Per-LED brightness and hue-vector sums for this frame.
    acc_v: [f32; LED_COUNT],
    acc_x: [f32; LED_COUNT],
    acc_y: [f32; LED_COUNT],
}

impl Ricochet {
    pub fn new() -> Self {
        let comet = Comet {
            alive: false,
            entering: false,
            respawn_at: 0,
            x: WORLD_CX,
            y: WORLD_CENTROID_Y,
            vx: 0.0,
            vy: 0.0,
            energy: 0.0,
            hue: 0.0,
            radius: HEAD_R_MAX_MM,
            speed: 0.0,
            trail: [(WORLD_CX, WORLD_CENTROID_Y); TRAIL_LEN],
            trail_head: 0,
        };
        Ricochet {
            prev_ms: 0,
            rng:     Rng::new(1),
            comets:  [comet; MAX_COMETS],
            sparks:  [Spark { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, life: 0.0, chue: 1.0, shue: 0.0 }; SPARK_POOL],
            acc_v: [0.0; LED_COUNT],
            acc_x: [0.0; LED_COUNT],
            acc_y: [0.0; LED_COUNT],
        }
    }

    /// Launches comet `i` inward from a random point on a random wall, in a new color.
    fn launch(&mut self, i: usize) {
        let edge = (self.rng.f32() * 3.0) as usize;
        let t = 0.12 + self.rng.f32() * 0.76; // stay off the corners
        let (ex, ey, n) = match edge {
            0 => (lerp(WORLD_LEFT, WORLD_RIGHT, t), WORLD_TOP, N_TOP),
            1 => (lerp(WORLD_LEFT, WORLD_CX, t), lerp(WORLD_TOP, WORLD_BOT, t), N_LEFT),
            _ => (lerp(WORLD_RIGHT, WORLD_CX, t), lerp(WORLD_TOP, WORLD_BOT, t), N_RIGHT),
        };
        let ang = n.1.atan2(n.0) + (self.rng.f32() - 0.5) * 2.0 * ENTRY_SPREAD;
        let hue = self.rng.f32() * 360.0;
        // Fast comets are small, slow ones big.
        let speed_frac = self.rng.f32();
        let sp = (SPEED_MIN_MM_S + speed_frac * (SPEED_MAX_MM_S - SPEED_MIN_MM_S)) / 1000.0;
        let radius = lerp(HEAD_R_MAX_MM, HEAD_R_MIN_MM, speed_frac);
        // Start one radius outside the wall.
        let px = ex - n.0 * radius;
        let py = ey - n.1 * radius;
        let c = &mut self.comets[i];
        c.alive = true;
        c.entering = true;
        c.radius = radius;
        c.speed = sp;
        c.x = px;
        c.y = py;
        c.vx = ang.cos() * sp;
        c.vy = ang.sin() * sp;
        c.energy = 1.0;
        c.hue = hue;
        c.trail_head = 0;
        for t in c.trail.iter_mut() {
            *t = (px, py);
        }
    }

    /// Moves comet `i` by `dt` ms, bouncing it off any wall it crosses.
    fn step_comet(&mut self, i: usize, dt: f32, t_ms: u32) {
        let Ricochet { comets, sparks, rng, .. } = self;
        let c = &mut comets[i];
        c.x += c.vx * dt;
        c.y += c.vy * dt;

        if c.entering {
            let (d0, d1, d2) = wall_dists(c.x, c.y);
            if d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0 {
                c.entering = false;
            }
        } else {
            for _ in 0..4 {
                let (d0, d1, d2) = wall_dists(c.x, c.y);
                let (mut md, mut n) = (d0, N_TOP);
                if d1 < md {
                    md = d1;
                    n = N_LEFT;
                }
                if d2 < md {
                    md = d2;
                    n = N_RIGHT;
                }
                if md >= 0.0 {
                    break;
                }

                // Move back to the wall and reflect the velocity.
                c.x += -md * n.0;
                c.y += -md * n.1;
                let vn = c.vx * n.0 + c.vy * n.1;
                c.vx -= 2.0 * vn * n.0;
                c.vy -= 2.0 * vn * n.1;

                let a = (rng.f32() - 0.5) * 2.0 * BOUNCE_PERTURB;
                let (sa, ca) = a.sin_cos();
                let (vx, vy) = (c.vx, c.vy);
                c.vx = vx * ca - vy * sa;
                c.vy = vx * sa + vy * ca;

                spawn_sparks(sparks, rng, c, (SPARKS_PER_BOUNCE * c.energy) as usize, n);
                c.energy *= ENERGY_DECAY;
            }
        }

        c.trail_head = (c.trail_head + 1) % TRAIL_LEN;
        c.trail[c.trail_head] = (c.x, c.y);

        if c.energy < ENERGY_MIN {
            c.alive = false;
            let delay = RESPAWN_MIN_MS + (rng.f32() * (RESPAWN_MAX_MS - RESPAWN_MIN_MS) as f32) as u32;
            c.respawn_at = t_ms.wrapping_add(delay);
        }
    }

    fn update_sparks(&mut self, dt: f32) {
        let drag = (1.0 - SPARK_DRAG * dt).max(0.0);
        for s in self.sparks.iter_mut() {
            if s.life > 0.0 {
                s.vy += SPARK_GRAVITY * dt;
                s.vx *= drag;
                s.vy *= drag;
                s.x += s.vx * dt;
                s.y += s.vy * dt;
                s.life -= dt / SPARK_LIFE_MS;
            }
        }
    }

    /// Adds one dot to the LEDs it covers.
    fn scatter(&mut self, d: &Dot) {
        let (acc_v, acc_x, acc_y) = (&mut self.acc_v, &mut self.acc_x, &mut self.acc_y);
        grid::for_each_near(d.x, d.y, d.reach, |k| {
            let led = &LED_MAP[k];
            let dx = led.wx - d.x;
            let dy = led.wy - d.y;
            let c = ((1.0 - (dx * dx + dy * dy) * d.inv_r2) * EDGE_SHARP).min(1.0);
            if c > 0.0 {
                let cw = c * d.w;
                acc_v[k] += cw;
                acc_x[k] += cw * d.chue;
                acc_y[k] += cw * d.shue;
            }
        });
    }
}

/// Signed distance from (x, y) to the top, left and right walls; negative is outside.
fn wall_dists(x: f32, y: f32) -> (f32, f32, f32) {
    (
        y - WORLD_TOP,
        (x - WORLD_LEFT) * N_LEFT.0 + (y - WORLD_TOP) * N_LEFT.1,
        (x - WORLD_RIGHT) * N_RIGHT.0 + (y - WORLD_TOP) * N_RIGHT.1,
    )
}

/// Sprays up to `count` sparks from comet `c`, fanned around the wall's inward normal `n`.
fn spawn_sparks(sparks: &mut [Spark; SPARK_POOL], rng: &mut Rng, c: &Comet, count: usize, n: (f32, f32)) {
    let radius = c.radius * (0.25 + 0.75 * c.energy);
    let (shue, chue) = c.hue.to_radians().sin_cos();
    let base = n.1.atan2(n.0);
    let mut spawned = 0;
    for s in sparks.iter_mut() {
        if spawned >= count {
            break;
        }
        if s.life <= 0.0 {
            // Start on the comet's edge, so the spark is not hidden inside the bright head.
            let ang = base + (rng.f32() - 0.5) * 2.0 * SPARK_FAN;
            let (dx, dy) = (ang.cos(), ang.sin());
            let sp = c.speed * SPARK_SPEED_FRAC * (0.7 + rng.f32() * 0.6);
            *s = Spark {
                x: c.x + dx * radius,
                y: c.y + dy * radius,
                vx: dx * sp,
                vy: dy * sp,
                life: 1.0,
                chue,
                shue,
            };
            spawned += 1;
        }
    }
}

impl Pattern for Ricochet {
    fn on_enter(&mut self, t_ms: u32) {
        // Reseed, clear the sparks, and stagger the first launches.
        self.prev_ms = t_ms;
        self.rng = Rng::new((t_ms ^ 0x9E37_79B9) | 1);
        for i in 0..MAX_COMETS {
            self.comets[i].alive = false;
            let delay = (self.rng.f32() * STAGGER_MS) as u32;
            self.comets[i].respawn_at = t_ms.wrapping_add(delay);
        }
        self.comets[0].respawn_at = t_ms; // one comet right away
        for s in self.sparks.iter_mut() {
            s.life = 0.0;
        }
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt = (t_ms.wrapping_sub(self.prev_ms) as f32).min(MAX_DT_MS);
        self.prev_ms = t_ms;

        for i in 0..MAX_COMETS {
            if self.comets[i].alive {
                self.step_comet(i, dt, t_ms);
            } else if t_ms.wrapping_sub(self.comets[i].respawn_at) as i32 >= 0 {
                self.launch(i);
            }
        }
        self.update_sparks(dt);

        // This frame's dots: each comet's head and trail, then the live sparks.
        let mut dots = [Dot { x: 0.0, y: 0.0, inv_r2: 0.0, w: 0.0, chue: 0.0, shue: 0.0, reach: 0 }; MAX_COMETS * TRAIL_LEN + SPARK_POOL];
        let mut nd = 0;
        for i in 0..MAX_COMETS {
            if !self.comets[i].alive {
                continue;
            }
            let (shue, chue) = self.comets[i].hue.to_radians().sin_cos();
            let energy = self.comets[i].energy;
            let radius = self.comets[i].radius;
            let head = self.comets[i].trail_head;
            for j in 0..TRAIL_LEN {
                let idx = (head + TRAIL_LEN - j) % TRAIL_LEN; // j = 0 is the head
                let (tx, ty) = self.comets[i].trail[idx];
                let f = j as f32 / TRAIL_LEN as f32;
                let (r, w) = if j == 0 {
                    (radius * (0.25 + 0.75 * energy), energy)
                } else {
                    (TRAIL_R_MM * (1.0 - TAIL_TAPER * f), (1.0 - f) * energy)
                };
                dots[nd] = Dot {
                    x: tx,
                    y: ty,
                    inv_r2: 1.0 / (r * r),
                    w,
                    chue,
                    shue,
                    reach: (r / CELL_MM).ceil() as usize,
                };
                nd += 1;
            }
        }
        for s in &self.sparks {
            if s.life > 0.0 {
                dots[nd] = Dot {
                    x: s.x,
                    y: s.y,
                    inv_r2: 1.0 / (SPARK_R_MM * SPARK_R_MM),
                    w: (s.life * SPARK_BRIGHT).min(1.0),
                    chue: s.chue,
                    shue: s.shue,
                    reach: (SPARK_R_MM / CELL_MM).ceil() as usize,
                };
                nd += 1;
            }
        }

        for k in 0..LED_COUNT {
            self.acc_v[k] = 0.0;
            self.acc_x[k] = 0.0;
            self.acc_y[k] = 0.0;
        }
        for d in dots.iter().take(nd) {
            self.scatter(d);
        }
        for (k, slot) in out.iter_mut().enumerate() {
            let v = self.acc_v[k].min(1.0);
            if v <= 0.0 {
                *slot = [0, 0, 0];
                continue;
            }
            let hue = self.acc_y[k].atan2(self.acc_x[k]).to_degrees();
            let sat = BASE_SAT * (1.0 - v * v * HEAD_WHITEN);
            *slot = hsv(wrap360(hue), sat, v);
        }
    }
}
