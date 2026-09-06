use crate::patterns::{Frame, Pattern, hsv, lerp, wrap360};
use crate::led::map::{Led, WORLD_TOP, WORLD_BOT, WORLD_CX, WORLD_CENTROID_Y, LED_COUNT};

// Ricochet - up to a few comets loose inside the triangle, bouncing off the three walls like
// Pong and dragging fading trails. Each bounce throws off a little shower of sparks and costs
// the comet energy, so it shrinks and dims with every hit until it fizzles out - then, after a
// random pause, a fresh comet launches from a new spot in a new color. Sparse by design.
//
// Stateful (per-comet position/velocity/energy/trail, a shared spark pool), integrated over
// real elapsed time and reset cleanly on re-entry. Rendered by SCATTER: each dot (head, trail
// point, spark) splats only onto the LEDs in the nearby cells of a fixed spatial grid, instead
// of testing every LED against every dot - the difference between 30 fps and a slideshow here.

// ============================== Tuning knobs ==============================

const MAX_COMETS:     usize = 4;    // most comets alive at once
const RESPAWN_MIN_MS: u32 = 500;    // pause after a comet dies before it relaunches
const RESPAWN_MAX_MS: u32 = 2500;
const STAGGER_MS:     f32 = 3000.0; // spread of the comets' first launches on entry

const SPEED_MIN_MM_S: f32 = 50.0;  // slowest comet (each picks its own speed in this range)
const SPEED_MAX_MM_S: f32 = 200.0; // fastest comet
const ENERGY_DECAY:   f32 = 0.8;   // energy multiplier per bounce (lower = dies sooner)
const ENERGY_MIN:     f32 = 0.1;   // below this the comet fizzles and (later) relaunches
const BOUNCE_PERTURB: f32 = 0.15;  // random angle nudge (rad) per bounce, so it never loops
const ENTRY_SPREAD:   f32 = 0.9;   // max angle (rad) off straight-in when entering from a wall

const HEAD_R_MIN_MM: f32 = 50.0;  // radius of the fastest comet (small, streaky)
const HEAD_R_MAX_MM: f32 = 95.0;  // radius of the slowest comet (big, lumbering)
const EDGE_SHARP:  f32   = 2.0;   // >1 sharpens the dot edge (a defined ball, less blobby)
const TRAIL_LEN:   usize = 24;    // trail history length per comet
const TRAIL_R_MM:  f32   = 18.0;  // tail width just behind the ball (narrows toward the tip)
const TAIL_TAPER:  f32   = 0.8;   // how much the tail narrows from head to tip (0 = even width)
const BASE_SAT:    f32   = 0.85;  // comet color saturation
const HEAD_WHITEN: f32   = 0.7;   // bright cores desaturate toward white (the glint)

const SPARK_POOL:        usize = 28;    // max sparks alive at once (shared by all comets)
const SPARKS_PER_BOUNCE: f32   = 10.0;  // sparks flung per bounce at full energy (scales down)
const SPARK_SPEED_FRAC:  f32   = 1.5;   // spark launch speed as a fraction of the ball's speed
const SPARK_DRAG:        f32   = 0.0025;// per-ms velocity drag - sparks fly out, then slow down
const SPARK_FAN:         f32   = 1.5;   // half-angle (rad) sparks fan out as they spray off the wall
const SPARK_LIFE_MS:     f32   = 900.0; // spark lifetime
const SPARK_R_MM:        f32   = 15.0;  // must exceed LED spacing (~10mm) or sparks fall between LEDs
const SPARK_BRIGHT:      f32   = 2.5;   // >1 keeps sparks full-bright (white-hot like the ball) before fading
const SPARK_GRAVITY:     f32   = 0.000_15; // gentle downward pull (mm/ms^2)

const MAX_DT_MS:      f32 = 60.0; // clamp the integration step (guards against long gaps)
const REENTRY_GAP_MS: u32 = 500;  // a render gap longer than this means we just (re)entered

// Triangle: the two top corners' x, and the three walls' inward unit normals (point-down,
// roughly equilateral). Left/right walls use a top corner (at WORLD_TOP) as their reference.
const V_LEFT_X:  f32 = 10.0;
const V_RIGHT_X: f32 = 2.0 * WORLD_CX - 10.0;
const N_TOP:   (f32, f32) = (0.0, 1.0);
const N_LEFT:  (f32, f32) = (0.866_025_4, -0.5);
const N_RIGHT: (f32, f32) = (-0.866_025_4, -0.5);

// Each dot splats over the shared grid's cells within its radius (its `reach`), so any dot
// size renders fully.
use crate::led::grid::{self, CELL_MM};

#[derive(Clone, Copy)]
struct Comet {
    alive:      bool,
    entering:   bool, // flying in from outside the wall - no bouncing until fully inside
    respawn_at: u32, // t_ms to launch when not alive
    x:      f32,
    y:      f32,
    vx:     f32,
    vy:     f32,
    energy: f32,
    hue:    f32,
    radius: f32, // head-ball radius at full energy (set from speed at launch: fast = small)
    speed:  f32, // velocity magnitude (mm/ms), constant over life; sparks inherit it
    trail:      [(f32, f32); TRAIL_LEN],
    trail_head: usize,
}

#[derive(Clone, Copy)]
struct Spark {
    x:    f32,
    y:    f32,
    vx:   f32,
    vy:   f32,
    life: f32,  // 1 at birth -> 0 dead
    chue: f32,  // color as a wheel vector, inherited from the comet
    shue: f32,
}

// A soft round dot to draw: center, 1/radius^2, brightness weight, and color as a wheel vector.
#[derive(Clone, Copy)]
struct Dot {
    x:      f32,
    y:      f32,
    inv_r2: f32,
    w:      f32,
    chue:   f32,
    shue:   f32,
    reach:  usize, // how many grid cells out to splat (from the dot's radius)
}

pub struct Ricochet {
    prev_ms: u32,
    active:  bool,
    rng:     u32,
    comets:  [Comet; MAX_COMETS],
    sparks:  [Spark; SPARK_POOL],
    // Per-LED accumulators (brightness + color as a wheel vector), reused each frame.
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
            active:  false,
            rng:     1,
            comets:  [comet; MAX_COMETS],
            sparks:  [Spark { x: 0.0, y: 0.0, vx: 0.0, vy: 0.0, life: 0.0, chue: 1.0, shue: 0.0 }; SPARK_POOL],
            acc_v: [0.0; LED_COUNT],
            acc_x: [0.0; LED_COUNT],
            acc_y: [0.0; LED_COUNT],
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

    /// Random f32 in [0, 1).
    fn randf(&mut self) -> f32 {
        (self.next_rng() >> 8) as f32 / 16_777_216.0
    }

    /// Launch comet `i`: enter from a random point on a random wall, aimed inward, at full
    /// energy in a new color.
    fn launch(&mut self, i: usize) {
        let edge = (self.randf() * 3.0) as usize;
        let t = 0.12 + self.randf() * 0.76; // stay off the corners
        let (ex, ey, n) = match edge {
            0 => (lerp(V_LEFT_X, V_RIGHT_X, t), WORLD_TOP, N_TOP),
            1 => (lerp(V_LEFT_X, WORLD_CX, t), lerp(WORLD_TOP, WORLD_BOT, t), N_LEFT),
            _ => (lerp(V_RIGHT_X, WORLD_CX, t), lerp(WORLD_TOP, WORLD_BOT, t), N_RIGHT),
        };
        let ang = n.1.atan2(n.0) + (self.randf() - 0.5) * 2.0 * ENTRY_SPREAD;
        let hue = self.randf() * 360.0;
        // One draw sets both speed and size: fast comets are small and streaky, slow ones big.
        let speed_frac = self.randf();
        let sp = (SPEED_MIN_MM_S + speed_frac * (SPEED_MAX_MM_S - SPEED_MIN_MM_S)) / 1000.0;
        let radius = lerp(HEAD_R_MAX_MM, HEAD_R_MIN_MM, speed_frac);
        // Start one ball-radius OUTSIDE the wall, aimed inward, so it drifts into frame.
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

    /// Spray up to `count` sparks off the impact: spread along the contact (the ball's width at
    /// the wall) and fanned out along the inward normal `n`, in the comet's color.
    fn spawn_sparks(&mut self, i: usize, count: usize, n: (f32, f32)) {
        let c = self.comets[i];
        let radius = c.radius * (0.25 + 0.75 * c.energy);
        let (shue, chue) = c.hue.to_radians().sin_cos();
        let base = n.1.atan2(n.0); // inward normal - sparks spray into the triangle
        let mut spawned = 0;
        for k in 0..SPARK_POOL {
            if spawned >= count {
                break;
            }
            if self.sparks[k].life <= 0.0 {
                // Spawn on the ball's inward-facing edge (not its buried center) and fly outward,
                // so the spark starts in clear space instead of inside the ball's bright core.
                let ang = base + (self.randf() - 0.5) * 2.0 * SPARK_FAN;
                let (dx, dy) = (ang.cos(), ang.sin());
                let sp = c.speed * SPARK_SPEED_FRAC * (0.7 + self.randf() * 0.6); // ~the ball's speed
                self.sparks[k] = Spark {
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

    /// Advance comet `i` by `dt` ms, reflecting off any wall it crosses (shedding sparks and
    /// energy each bounce). Schedules a relaunch once it's spent.
    fn step_comet(&mut self, i: usize, dt: f32, t_ms: u32) {
        self.comets[i].x += self.comets[i].vx * dt;
        self.comets[i].y += self.comets[i].vy * dt;

        if self.comets[i].entering {
            // Flying in from outside the wall - don't bounce until the center is fully inside.
            let x = self.comets[i].x;
            let y = self.comets[i].y;
            let d0 = y - WORLD_TOP;
            let d1 = (x - V_LEFT_X) * N_LEFT.0 + (y - WORLD_TOP) * N_LEFT.1;
            let d2 = (x - V_RIGHT_X) * N_RIGHT.0 + (y - WORLD_TOP) * N_RIGHT.1;
            if d0 >= 0.0 && d1 >= 0.0 && d2 >= 0.0 {
                self.comets[i].entering = false;
            }
        } else {
            for _ in 0..4 {
                let x = self.comets[i].x;
                let y = self.comets[i].y;
                let d0 = y - WORLD_TOP;
                let d1 = (x - V_LEFT_X) * N_LEFT.0 + (y - WORLD_TOP) * N_LEFT.1;
                let d2 = (x - V_RIGHT_X) * N_RIGHT.0 + (y - WORLD_TOP) * N_RIGHT.1;

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

                // Push back to the wall, mirror the velocity across its normal.
                self.comets[i].x += -md * n.0;
                self.comets[i].y += -md * n.1;
                let vn = self.comets[i].vx * n.0 + self.comets[i].vy * n.1;
                self.comets[i].vx -= 2.0 * vn * n.0;
                self.comets[i].vy -= 2.0 * vn * n.1;

                // Random nudge so the path never falls into a boring loop.
                let a = (self.randf() - 0.5) * 2.0 * BOUNCE_PERTURB;
                let (sa, ca) = a.sin_cos();
                let (vx, vy) = (self.comets[i].vx, self.comets[i].vy);
                self.comets[i].vx = vx * ca - vy * sa;
                self.comets[i].vy = vx * sa + vy * ca;

                // Spray sparks off the impact, then lose energy.
                let energy = self.comets[i].energy;
                self.spawn_sparks(i, (SPARKS_PER_BOUNCE * energy) as usize, n);
                self.comets[i].energy *= ENERGY_DECAY;
            }
        }

        // Record the head into the trail ring.
        self.comets[i].trail_head = (self.comets[i].trail_head + 1) % TRAIL_LEN;
        let (th, cx, cy) = (self.comets[i].trail_head, self.comets[i].x, self.comets[i].y);
        self.comets[i].trail[th] = (cx, cy);

        // Fizzled out -> schedule a relaunch after a random pause.
        if self.comets[i].energy < ENERGY_MIN {
            self.comets[i].alive = false;
            let delay = RESPAWN_MIN_MS + (self.randf() * (RESPAWN_MAX_MS - RESPAWN_MIN_MS) as f32) as u32;
            self.comets[i].respawn_at = t_ms.wrapping_add(delay);
        }
    }

    fn update_sparks(&mut self, dt: f32) {
        let drag = (1.0 - SPARK_DRAG * dt).max(0.0); // fly out, then slow down
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

    /// Splat one dot onto the LEDs in the grid cells within its reach.
    fn scatter(&mut self, leds: &[Led], d: &Dot) {
        let (acc_v, acc_x, acc_y) = (&mut self.acc_v, &mut self.acc_x, &mut self.acc_y);
        grid::for_each_near(d.x, d.y, d.reach, |k| {
            let led = &leds[k];
            let dx = led.wx - d.x;
            let dy = led.wy - d.y;
            // Sharpen the edge so it reads as a solid ball, not a soft blob.
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

impl Pattern for Ricochet {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        // Real elapsed time since the last render; a long gap means we just (re)entered the
        // pattern, so reseed, clear the sparks, and stagger the comets' first launches.
        let reset = !self.active || t_ms.wrapping_sub(self.prev_ms) > REENTRY_GAP_MS;
        let dt = if reset { 0.0 } else { t_ms.wrapping_sub(self.prev_ms) as f32 };
        self.prev_ms = t_ms;
        if reset {
            self.rng = (t_ms ^ 0x9E37_79B9) | 1;
            self.active = true;
            for i in 0..MAX_COMETS {
                self.comets[i].alive = false;
                let delay = (self.randf() * STAGGER_MS) as u32;
                self.comets[i].respawn_at = t_ms.wrapping_add(delay);
            }
            self.comets[0].respawn_at = t_ms; // one comet right away
            for s in self.sparks.iter_mut() {
                s.life = 0.0;
            }
        }
        let dt = dt.min(MAX_DT_MS);

        // Advance / (re)launch each comet, then the sparks.
        for i in 0..MAX_COMETS {
            if self.comets[i].alive {
                self.step_comet(i, dt, t_ms);
            } else if t_ms >= self.comets[i].respawn_at {
                self.launch(i);
            }
        }
        self.update_sparks(dt);

        // Gather this frame's dots (each comet's head + trail, plus live sparks).
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
                let idx = (head + TRAIL_LEN - j) % TRAIL_LEN; // j = 0 is the head (newest)
                let (tx, ty) = self.comets[i].trail[idx];
                let f = j as f32 / TRAIL_LEN as f32;
                let (r, w) = if j == 0 {
                    (radius * (0.25 + 0.75 * energy), energy) // the ball
                } else {
                    (TRAIL_R_MM * (1.0 - TAIL_TAPER * f), (1.0 - f) * energy) // the narrowing tail
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
                    w: (s.life * SPARK_BRIGHT).min(1.0), // full-bright, then fades over the last stretch
                    chue: s.chue,
                    shue: s.shue,
                    reach: (SPARK_R_MM / CELL_MM).ceil() as usize,
                };
                nd += 1;
            }
        }

        // Clear the accumulators, scatter every dot onto its nearby LEDs, then resolve to color.
        for k in 0..leds.len() {
            self.acc_v[k] = 0.0;
            self.acc_x[k] = 0.0;
            self.acc_y[k] = 0.0;
        }
        for d in dots.iter().take(nd) {
            self.scatter(leds, d);
        }
        for (k, slot) in out.iter_mut().take(leds.len()).enumerate() {
            let v = self.acc_v[k].min(1.0);
            if v <= 0.0 {
                *slot = [0, 0, 0];
                continue;
            }
            let hue = self.acc_y[k].atan2(self.acc_x[k]).to_degrees();
            let sat = BASE_SAT * (1.0 - v * v * HEAD_WHITEN); // bright cores glint white
            *slot = hsv(wrap360(hue), sat, v);
        }
    }
}
