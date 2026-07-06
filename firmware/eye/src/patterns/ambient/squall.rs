use crate::patterns::{Frame, Pattern, lerp};
use crate::led::map::{Led, WORLD_BOT, WORLD_CX, WORLD_H, WORLD_TOP};
use core::f32::consts::{PI, TAU};

// Tunables - dial these in the previewer. The pattern has no top or bottom: every
// layer is isotropic, like open ocean seen from a plane or a cloud deck from below.

// Horizontal bounds: map.rs publishes only vertical extents, but the fixture is an
// equilateral point-down triangle, so the top edge half-span is WORLD_H / sqrt(3).
const WORLD_HALF_W: f32 = 248.0;
const WORLD_LEFT:   f32 = WORLD_CX - WORLD_HALF_W;
const WORLD_RIGHT:  f32 = WORLD_CX + WORLD_HALF_W;

// Water floor: resting level on the ramp plus a slow in-place boil so the dark
// stretches between blooms never sit static.
const WATER_FLOOR: f32 = 0.32;
const BOIL_DEPTH:  f32 = 0.10;
const BOIL_CELL_MM: f32 = 140.0;    // spatial scale of the churn
const BOIL_PERIOD_MS:  u32 = 7_300; // two incommensurate phases so the churn
const BOIL2_PERIOD_MS: u32 = 4_700; // reads as boiling, not a sweep

// Upwellings: hash-scheduled blooms that swell out of the dark, spread, and dissolve,
// peak brightness falling as the radius grows so the energy feels like it diffuses.
const BLOOM_SLOTS: usize = 8;
const BLOOM_PERIOD_MS:  u32 = 4_600; // base lifetime; each slot staggers longer
const BLOOM_STAGGER_MS: u32 = 733;   // per-slot period offset keeps respawns unsynced
const BLOOM_R0_MM: f32 = 45.0;  // radius at birth...
const BLOOM_R1_MM: f32 = 175.0; // ...and at full spread
const BLOOM_PEAK:  f32 = 0.6;   // energy a young bloom adds at its center
const BLOOM_RIM:   f32 = 0.55;  // 0 = fades in place; 1 = dies as an expanding ring

// Whitecaps: hash-twinkled froth where bloom energy crests past the threshold.
const FOAM_THRESH: f32 = 0.8;
const FOAM_GAIN:   f32 = 2.5;      // how fast froth saturates past the threshold
const FOAM_PERIOD_MS: u32 = 530;   // twinkle period

// Clouds: pale drifting masses with real state - position and velocity, easing toward
// a new random heading at random intervals, exiting one edge to re-enter at another.
const CLOUD_COUNT: usize = 3;
const CLOUD_LOBES: usize = 3;        // overlapping soft lobes keep the outline indistinct
const CLOUD_SPEED_MIN: f32 = 12.0;   // mm/s drift
const CLOUD_SPEED_MAX: f32 = 40.0;
const CLOUD_TURN_RATE: f32 = 0.8;    // 1/s - how quickly velocity eases toward its target
const CLOUD_RETARGET_MIN_MS: u32 = 3_000; // how long a heading holds before wandering
const CLOUD_RETARGET_MAX_MS: u32 = 9_000;
const CLOUD_LOBE_R_MM:   f32 = 90.0;
const CLOUD_LOBE_OFF_MM: f32 = 60.0; // lobe orbit radius around the cloud center
const CLOUD_BREATHE: f32 = 0.3;      // lobe radius swing while breathing
const CLOUD_BRIGHT:  f32 = 0.7;      // cover opacity at full density
const CLOUD_MARGIN_MM: f32 = 170.0;  // how far past an edge before re-entering elsewhere

// Lightning: irregular gaps, then a strike anchored to a cloud - a sheet flash lighting
// the cloud from within, sometimes carrying a crawler bolt arcing toward a sibling.
const STRIKE_GAP_MIN_MS: u32 = 6_000;
const STRIKE_GAP_MAX_MS: u32 = 20_000;
const STRIKE_LEN_MS: u32 = 480;      // whole strike including return strokes
const SUBFLASH_DECAY_MS: f32 = 70.0; // each sub-flash pops on and decays this fast
const BOLT_CHANCE: f32 = 0.35;       // otherwise the strike is sheet-only
const BOLT_SEGS: usize = 4;
const BOLT_KINK_MM: f32 = 45.0;      // perpendicular jitter of the crawler's joints
const BOLT_CORE_MM: f32 = 12.0;      // thin bright filament...
const BOLT_HALO_MM: f32 = 55.0;      // ...inside a soft glow
const SHEET_RADIUS_MM: f32 = 240.0;
const SHEET_GAIN: f32 = 0.9;

const FOAM_COLOR:  [f32; 3] = [235.0, 245.0, 250.0];
const CLOUD_COLOR: [f32; 3] = [150.0, 160.0, 178.0]; // pale blue-gray
const FLASH_COLOR: [f32; 3] = [225.0, 232.0, 255.0];

struct Cloud {
    x:   f32,
    y:   f32,
    vx:  f32,
    vy:  f32,
    tvx: f32, // target velocity the current one eases toward
    tvy: f32,
    retarget_start_ms: u32,
    retarget_len_ms:   u32,
    seed: u32, // decorrelates this cloud's lobe motion from its siblings
}

struct Strike {
    start_ms:    u32,
    cx:          f32, // anchor cloud position, frozen at ignition
    cy:          f32,
    subflash_ms: [u32; 3], // return-stroke onsets; u32::MAX marks an unused slot
    bolt:        Option<[(f32, f32); BOLT_SEGS + 1]>, // crawler polyline when present
}

/// Boiling sea under a storm: dark water bubbling with upwellings, pale clouds drifting
/// on wandering headings, and sparse lightning. Stateful (cloud simulation plus a seeded
/// PRNG) unlike the hash-scheduled ambients, so headings genuinely change over time.
pub struct Squall {
    clouds:       [Cloud; CLOUD_COUNT],
    strike:       Option<Strike>,
    gap_start_ms: u32, // countdown to the next strike
    gap_len_ms:   u32,
    rng:          u32,
    last_ms:      Option<u32>, // previous frame time; None until the first render
}

impl Squall {
    pub fn new() -> Self {
        let mut rng = 0x5EED_5EA5u32; // fixed seed keeps runs reproducible
        let clouds = core::array::from_fn(|_| {
            let x = lerp(WORLD_LEFT, WORLD_RIGHT, rand_f(&mut rng));
            let y = lerp(WORLD_TOP, WORLD_BOT, rand_f(&mut rng));
            let ang = rand_f(&mut rng) * TAU;
            let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rand_f(&mut rng));
            let (vx, vy) = (ang.cos() * speed, ang.sin() * speed);
            Cloud {
                x, y, vx, vy, tvx: vx, tvy: vy,
                retarget_start_ms: 0,
                retarget_len_ms:   0,
                seed: xorshift(&mut rng),
            }
        });
        Squall { clouds, strike: None, gap_start_ms: 0, gap_len_ms: 0, rng, last_ms: None }
    }

    /// Advance the drift simulation: ease each cloud toward its target velocity, pick a
    /// new target at irregular intervals, and re-enter from a fresh edge once fully off.
    fn update_clouds(&mut self, t_ms: u32, dt_s: f32) {
        let Squall { clouds, rng, .. } = self;
        let ease = (CLOUD_TURN_RATE * dt_s).min(1.0);
        for c in clouds.iter_mut() {
            if t_ms.wrapping_sub(c.retarget_start_ms) >= c.retarget_len_ms {
                let ang = rand_f(rng) * TAU;
                let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rand_f(rng));
                c.tvx = ang.cos() * speed;
                c.tvy = ang.sin() * speed;
                c.retarget_start_ms = t_ms;
                c.retarget_len_ms = roll_range(rng, CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
            }
            c.vx += (c.tvx - c.vx) * ease;
            c.vy += (c.tvy - c.vy) * ease;
            c.x += c.vx * dt_s;
            c.y += c.vy * dt_s;
            if c.x < WORLD_LEFT - CLOUD_MARGIN_MM || c.x > WORLD_RIGHT + CLOUD_MARGIN_MM
                || c.y < WORLD_TOP - CLOUD_MARGIN_MM || c.y > WORLD_BOT + CLOUD_MARGIN_MM
            {
                respawn(c, rng, t_ms);
            }
        }
    }

    /// Retire a finished strike, then ignite the next once the irregular gap elapses.
    fn update_strike(&mut self, t_ms: u32) {
        let expired = self.strike.as_ref()
            .is_some_and(|s| t_ms.wrapping_sub(s.start_ms) >= STRIKE_LEN_MS);
        if expired {
            self.strike = None;
            self.gap_start_ms = t_ms;
            self.gap_len_ms = roll_range(&mut self.rng, STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
        }
        if self.strike.is_none() && t_ms.wrapping_sub(self.gap_start_ms) >= self.gap_len_ms {
            self.strike = Some(self.ignite(t_ms));
        }
    }

    /// Build a strike anchored to a random cloud: 2-3 return strokes at ragged offsets,
    /// sometimes carrying a crawler bolt - a jagged polyline toward a second cloud.
    fn ignite(&mut self, t_ms: u32) -> Strike {
        let a = xorshift(&mut self.rng) as usize % CLOUD_COUNT;
        let (cx, cy) = (self.clouds[a].x, self.clouds[a].y);
        let sub2 = 80 + xorshift(&mut self.rng) % 140;
        let sub3 = if rand_f(&mut self.rng) < 0.6 {
            230 + xorshift(&mut self.rng) % 160
        } else {
            u32::MAX
        };
        let bolt = if rand_f(&mut self.rng) < BOLT_CHANCE {
            let b = (a + 1 + xorshift(&mut self.rng) as usize % (CLOUD_COUNT - 1)) % CLOUD_COUNT;
            let (dx, dy) = (self.clouds[b].x - cx, self.clouds[b].y - cy);
            let len = (dx * dx + dy * dy).sqrt().max(1.0);
            let (px, py) = (-dy / len, dx / len); // unit perpendicular, for the kinks
            let mut pts = [(0.0f32, 0.0f32); BOLT_SEGS + 1];
            for (i, pt) in pts.iter_mut().enumerate() {
                let f = i as f32 / BOLT_SEGS as f32;
                let kink = if i == 0 || i == BOLT_SEGS {
                    0.0 // endpoints sit on the clouds; only the joints jitter
                } else {
                    (rand_f(&mut self.rng) * 2.0 - 1.0) * BOLT_KINK_MM
                };
                *pt = (cx + dx * f + px * kink, cy + dy * f + py * kink);
            }
            Some(pts)
        } else {
            None
        };
        Strike { start_ms: t_ms, cx, cy, subflash_ms: [0, sub2, sub3], bolt }
    }
}

impl Pattern for Squall {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        // Fold each time term to its own period before the f32 cast (long-uptime precision).
        let boil1 = (t_ms % BOIL_PERIOD_MS) as f32 / BOIL_PERIOD_MS as f32 * TAU;
        let boil2 = (t_ms % BOIL2_PERIOD_MS) as f32 / BOIL2_PERIOD_MS as f32 * TAU;
        let foam_phase = (t_ms % FOAM_PERIOD_MS) as f32 / FOAM_PERIOD_MS as f32 * TAU;

        // Frame delta drives the cloud simulation. A long gap means this pattern was
        // off-screen: re-anchor the timers so everything doesn't fire at once on re-entry.
        let dt_s = match self.last_ms {
            Some(last) => {
                let dt_ms = t_ms.wrapping_sub(last);
                if dt_ms > 1_000 {
                    self.gap_start_ms = t_ms;
                    for c in &mut self.clouds {
                        c.retarget_start_ms = t_ms;
                    }
                }
                (dt_ms as f32 / 1000.0).min(0.1)
            }
            None => {
                // First frame: anchor the strike countdown so the storm opens quietly.
                self.gap_start_ms = t_ms;
                self.gap_len_ms = roll_range(&mut self.rng, STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
                0.0
            }
        };
        self.last_ms = Some(t_ms);
        self.update_clouds(t_ms, dt_s);
        self.update_strike(t_ms);

        // Bloom slots: each period, a slot hash-picks a spot near the triangle and lives
        // one bloom there - radius grows birth-to-death while a sin envelope swells and
        // fades the energy. (x, y, 1/r^2, amplitude, ring blend) per slot.
        let mut blooms = [(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32); BLOOM_SLOTS];
        for (k, b) in blooms.iter_mut().enumerate() {
            let period = BLOOM_PERIOD_MS + k as u32 * BLOOM_STAGGER_MS;
            let cycle = t_ms / period;
            let p = (t_ms % period) as f32 / period as f32;
            let h = cycle.wrapping_mul(2654435761) ^ (k as u32).wrapping_mul(0x9E37_79B9);
            let y = WORLD_TOP + ((h >> 10) % 1000) as f32 / 1000.0 * WORLD_H;
            let hw = (WORLD_BOT - y) / WORLD_H * WORLD_HALF_W + 40.0; // triangle width at y
            let x = WORLD_CX + ((h % 1000) as f32 / 1000.0 * 2.0 - 1.0) * hw;
            let r = lerp(BLOOM_R0_MM, BLOOM_R1_MM, p);
            *b = (x, y, 1.0 / (r * r), BLOOM_PEAK * (p * PI).sin(), BLOOM_RIM * p);
        }

        // Cloud lobes, flattened across clouds: each orbits its cloud center and breathes
        // its radius on seed-derived periods, so the silhouette slowly morphs. (x, y, 1/r^2).
        let mut lobes = [(0.0f32, 0.0f32, 0.0f32); CLOUD_COUNT * CLOUD_LOBES];
        for (ci, c) in self.clouds.iter().enumerate() {
            for j in 0..CLOUD_LOBES {
                let h = c.seed ^ (j as u32).wrapping_mul(0x9E37_79B9);
                let orbit_p = 9_000 + h % 7_000;
                let breathe_p = 3_500 + (h >> 8) % 3_000;
                let orbit = (t_ms % orbit_p) as f32 / orbit_p as f32 * TAU + (h >> 16) as f32;
                let breathe = (t_ms % breathe_p) as f32 / breathe_p as f32 * TAU + (h >> 12) as f32;
                let off = CLOUD_LOBE_OFF_MM * (0.35 + 0.65 * ((h >> 4) % 100) as f32 / 100.0);
                let r = CLOUD_LOBE_R_MM * (1.0 + CLOUD_BREATHE * breathe.sin());
                lobes[ci * CLOUD_LOBES + j] =
                    (c.x + orbit.cos() * off, c.y + orbit.sin() * off, 1.0 / (r * r));
            }
        }

        // Strike envelope: each return stroke pops on and decays fast.
        let strike = self.strike.as_ref();
        let mut flash_env = 0.0f32;
        if let Some(s) = strike {
            let el = t_ms.wrapping_sub(s.start_ms);
            for &on in &s.subflash_ms {
                if el >= on {
                    flash_env += (-((el - on) as f32) / SUBFLASH_DECAY_MS).exp();
                }
            }
            flash_env = flash_env.min(1.0);
        }

        for (i, led) in leds.iter().enumerate() {
            // Boiling floor: two crossed nested sines churn in place - no travel direction.
            let bx = led.wx / BOIL_CELL_MM * TAU;
            let by = led.wy / BOIL_CELL_MM * TAU;
            let boil = ((bx + (by * 0.83 + boil2).sin() + boil1).sin()
                + (by + (bx * 0.71 + boil1).sin() + boil2).sin()) * 0.5;
            let mut energy = WATER_FLOOR + BOIL_DEPTH * boil;

            // Upwellings: a young bloom is a filled swell; the rim blend morphs it toward
            // a ring as it ages, so it dies ghosting outward instead of switching off.
            for &(x, y, inv_r2, amp, ring) in &blooms {
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    let filled = (1.0 - q) * (1.0 - q);
                    let ringed = 4.0 * q * (1.0 - q);
                    energy += amp * lerp(filled, ringed, ring);
                }
            }

            // Water color, then froth where the crest breaks: hash-twinkled speckle.
            let mut c = water_ramp(energy);
            let excess = ((energy - FOAM_THRESH) * FOAM_GAIN).clamp(0.0, 1.0);
            if excess > 0.0 {
                let h = (led.chain_idx as u32).wrapping_mul(2654435761);
                let tw = (foam_phase + (h % 628) as f32 / 100.0).sin() * 0.5 + 0.5;
                c = mix(c, FOAM_COLOR, excess * tw * tw);
            }

            // Cloud cover: pale mass over the water, saturating where lobes overlap.
            let mut density = 0.0f32;
            for &(x, y, inv_r2) in &lobes {
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    density += (1.0 - q) * (1.0 - q);
                }
            }
            let density = density.min(1.0);
            c = mix(c, CLOUD_COLOR, density * CLOUD_BRIGHT);

            // Lightning: sheet glow brightest inside the cloud mass (lit from within),
            // plus the crawler's thin core and soft halo when this strike carries one.
            if let Some(s) = strike {
                if flash_env > 0.01 {
                    let q = ((led.wx - s.cx).powi(2) + (led.wy - s.cy).powi(2))
                        / (SHEET_RADIUS_MM * SHEET_RADIUS_MM);
                    let mut flash = SHEET_GAIN * (1.0 - q).max(0.0) * (0.35 + 0.65 * density);
                    if let Some(pts) = &s.bolt {
                        let d2 = polyline_d2(pts, led.wx, led.wy);
                        flash += (-d2 / (BOLT_CORE_MM * BOLT_CORE_MM)).exp()
                            + 0.4 * (-d2 / (BOLT_HALO_MM * BOLT_HALO_MM)).exp();
                    }
                    c = mix(c, FLASH_COLOR, (flash_env * flash).clamp(0.0, 1.0));
                }
            }

            out[i] = [c[0] as u8, c[1] as u8, c[2] as u8];
        }
    }
}

/// Re-enter just off a random edge, aimed at a random point in the canvas middle so
/// every entrance takes a fresh heading but always comes back on stage.
fn respawn(c: &mut Cloud, rng: &mut u32, t_ms: u32) {
    let along = rand_f(rng);
    let (x, y) = match xorshift(rng) % 4 {
        0 => (lerp(WORLD_LEFT, WORLD_RIGHT, along), WORLD_TOP - CLOUD_MARGIN_MM + 1.0),
        1 => (lerp(WORLD_LEFT, WORLD_RIGHT, along), WORLD_BOT + CLOUD_MARGIN_MM - 1.0),
        2 => (WORLD_LEFT - CLOUD_MARGIN_MM + 1.0, lerp(WORLD_TOP, WORLD_BOT, along)),
        _ => (WORLD_RIGHT + CLOUD_MARGIN_MM - 1.0, lerp(WORLD_TOP, WORLD_BOT, along)),
    };
    let tx = lerp(WORLD_LEFT, WORLD_RIGHT, 0.25 + 0.5 * rand_f(rng));
    let ty = lerp(WORLD_TOP, WORLD_BOT, 0.25 + 0.5 * rand_f(rng));
    let (dx, dy) = (tx - x, ty - y);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rand_f(rng));
    c.x = x;
    c.y = y;
    c.vx = dx / len * speed;
    c.vy = dy / len * speed;
    c.tvx = c.vx;
    c.tvy = c.vy;
    c.retarget_start_ms = t_ms;
    c.retarget_len_ms = roll_range(rng, CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
}

/// Storm-water ramp: abyssal black-blue up through navy and teal to foam white. The
/// resting floor sits in the navy band; only crests and lightning reach the top.
fn water_ramp(e: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.00, [0.0, 2.0, 10.0]),      // abyssal black-blue
        (0.35, [6.0, 22.0, 64.0]),     // deep navy
        (0.60, [12.0, 60.0, 120.0]),   // ocean blue
        (0.80, [26.0, 118.0, 148.0]),  // storm teal
        (0.92, [150.0, 205.0, 215.0]), // pale seafoam
        (1.00, [235.0, 245.0, 250.0]), // foam white
    ];
    let e = e.clamp(0.0, 1.0);
    for pair in STOPS.windows(2) {
        let (e0, c0) = pair[0];
        let (e1, c1) = pair[1];
        if e <= e1 {
            let t = (e - e0) / (e1 - e0);
            return mix(c0, c1, t);
        }
    }
    STOPS[STOPS.len() - 1].1
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [lerp(a[0], b[0], t), lerp(a[1], b[1], t), lerp(a[2], b[2], t)]
}

/// Squared distance from (x, y) to the nearest segment of a polyline.
fn polyline_d2(pts: &[(f32, f32)], x: f32, y: f32) -> f32 {
    let mut best = f32::MAX;
    for seg in pts.windows(2) {
        let (ax, ay) = seg[0];
        let (bx, by) = seg[1];
        let (ex, ey) = (bx - ax, by - ay);
        let t = (((x - ax) * ex + (y - ay) * ey) / (ex * ex + ey * ey).max(1.0)).clamp(0.0, 1.0);
        let (dx, dy) = (x - ax - ex * t, y - ay - ey * t);
        best = best.min(dx * dx + dy * dy);
    }
    best
}

fn xorshift(state: &mut u32) -> u32 {
    let mut x = *state;
    x ^= x << 13;
    x ^= x >> 17;
    x ^= x << 5;
    *state = x;
    x
}

/// Uniform random f32 in [0, 1).
fn rand_f(state: &mut u32) -> f32 {
    (xorshift(state) >> 8) as f32 / 16_777_216.0
}

/// Uniform random duration in [min_ms, max_ms).
fn roll_range(rng: &mut u32, min_ms: u32, max_ms: u32) -> u32 {
    min_ms + xorshift(rng) % (max_ms - min_ms)
}
