use crate::audio::Audio;
use crate::led::geom::{LATTICE_ROW_H, LATTICE_SIDE, LATTICE_TOP_Y};
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{LED_COUNT, LED_MAP, WORLD_BOT, WORLD_CX, WORLD_H, WORLD_TOP};
use crate::led::world::{half_width_at, WORLD_LEFT, WORLD_RIGHT};
use crate::patterns::{
    cycle, hash01, knuth_hash, lerp, mix_rgb, phase_phasors, ramp, random_u32, time_step_ms, Frame,
    Pattern, Phasor, Rng, PHASE_STEPS,
};
use core::f32::consts::{PI, TAU};

// Water floor: the resting level, plus a slow boil in place.
const WATER_FLOOR: f32 = 0.40;
const BOIL_DEPTH: f32 = 0.10;
const BOIL_CELL_MM: f32 = 140.0; // size of the churn
const BOIL_ANGLE_PER_MM: f32 = TAU / BOIL_CELL_MM;
// Two unrelated periods, so it boils rather than sweeps.
const BOIL_PERIOD_MS: u32 = 4_900;
const BOIL2_PERIOD_MS: u32 = 3_100;
// Each axis' modulating sine, relative to the cell size; off-integer so the two never line up.
const BOIL_CROSS_Y: f32 = 0.83;
const BOIL_CROSS_X: f32 = 0.71;

// Upwellings: blooms that swell out of the dark, spread and dissolve.
const BLOOM_SLOTS: usize = 8;
const BLOOM_PERIOD_MS: u32 = 4_600; // base lifetime
const BLOOM_STAGGER_MS: u32 = 733; // added per slot, so they respawn out of step
const BLOOM_R0_MM: f32 = 45.0; // radius at birth
const BLOOM_R1_MM: f32 = 175.0; // radius at full spread
const BLOOM_PEAK: f32 = 0.6; // energy a young bloom adds at its center
const BLOOM_RIM: f32 = 0.55; // 0 fades in place, 1 dies as an expanding ring

// Whitecaps: twinkling froth where the energy passes the threshold.
const FOAM_THRESH: f32 = 0.8;
const FOAM_GAIN: f32 = 2.5; // how fast froth saturates past the threshold
const FOAM_PERIOD_MS: u32 = 530; // twinkle period

// Clouds: pale masses that drift, turn toward a new heading now and then, and come back
// in from another edge after leaving.
const CLOUD_COUNT: usize = 3;
const CLOUD_LOBES: usize = 3; // overlapping soft lobes per cloud
const CLOUD_SPEED_MIN: f32 = 12.0; // mm/s
const CLOUD_SPEED_MAX: f32 = 40.0;
const CLOUD_TURN_RATE: f32 = 0.8; // 1/s, how fast velocity eases to its target
const CLOUD_RETARGET_MIN_MS: u32 = 3_000; // how long a heading holds
const CLOUD_RETARGET_MAX_MS: u32 = 9_000;
const CLOUD_LOBE_R_MM: f32 = 90.0;
const CLOUD_LOBE_OFF_MM: f32 = 60.0; // lobe orbit radius around the cloud center
const CLOUD_BREATHE: f32 = 0.3; // lobe radius swing
const CLOUD_ORBIT_MIN_MS: u32 = 9_000; // time for a lobe to circle its cloud
const CLOUD_ORBIT_MAX_MS: u32 = 16_000;
const CLOUD_BREATHE_MIN_MS: u32 = 3_500; // time for a lobe to swell and shrink
const CLOUD_BREATHE_MAX_MS: u32 = 6_500;
const CLOUD_BRIGHT: f32 = 0.7; // opacity at full density
const CLOUD_MARGIN_MM: f32 = 170.0; // how far past an edge before re-entering

// Lightning: a white bolt builds along the tile edges one side at a time, flickers
// through a few return strokes, then dies.
const BOLT_SLOTS: usize = 2; // bolts that can be alive at once
const STRIKE_GAP_MIN_MS: u32 = 4_000; // per slot
const STRIKE_GAP_MAX_MS: u32 = 14_000;
const BOLT_SEGS_MIN: usize = 3; // bolt length, in tile sides
const BOLT_SEGS_MAX: usize = 8;
const BOLT_STEP_MS: u32 = 150; // time per tile side while building
const FLICKER_FADE_MS: f32 = 180.0; // decay of each return stroke
const BOLT_PULSES_MIN: usize = 2; // return strokes per strike
const BOLT_PULSES_MAX: usize = 5;
const PULSE_GAP_MIN_MS: u32 = 60; // between return strokes
const PULSE_GAP_MAX_MS: u32 = 420;
const BOLT_TAIL_MS: u32 = 1_400; // slot held after the last stroke; must cover the fade
const BOLT_CORE_MM: f32 = 12.0; // reaches the two LED rows either side of a tile edge
const BOLT_REACH_MM: f32 = 3.0 * BOLT_CORE_MM; // LEDs farther from the line are skipped
const BOLT_GAIN: f32 = 1.6; // LEDs near the line saturate to full white

// The bolts walk the tile-corner lattice. Neighbors as (row, index) steps: left, right, and
// the four diagonals.
const LATTICE_NEIGHBORS: [(i32, i32); 6] = [(0, -1), (0, 1), (-1, 0), (-1, 1), (1, -1), (1, 0)];
const LATTICE_LAST_ROW: i32 = 5; // row r's last index is LATTICE_LAST_ROW - r

const SLOT_MIX: u32 = 0x9E37_79B9; // spreads slot and lobe numbers apart in a hash

/// Water color by energy.
const WATER_RAMP: [(f32, [f32; 3]); 6] = [
    (0.00, [2.0, 8.0, 31.0]),      // deep blue-black
    (0.32, [9.0, 37.0, 97.0]),     // navy-blue
    (0.58, [17.0, 84.0, 149.0]),   // ocean blue
    (0.78, [33.0, 144.0, 169.0]),  // teal-turquoise
    (0.91, [150.0, 215.0, 222.0]), // pale aqua
    (1.00, [240.0, 249.0, 251.0]), // foam white
];
const FOAM_COLOR: [f32; 3] = [235.0, 245.0, 250.0];
const CLOUD_COLOR: [f32; 3] = [168.0, 178.0, 196.0]; // pale blue-gray
const FLASH_COLOR: [f32; 3] = [255.0, 255.0, 255.0];

struct Cloud {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    target_vx: f32,
    target_vy: f32,
    retarget_start_ms: u32,
    retarget_len_ms: u32,
    seed: u32, // makes this cloud's lobes move differently from the others'
}

impl Cloud {
    /// Puts the cloud just off a random edge, heading for a random point near the middle.
    fn respawn(&mut self, rng: &mut Rng, t_ms: u32) {
        let along = rng.f32();
        let (x, y) = match rng.below(4) {
            0 => (
                lerp(WORLD_LEFT, WORLD_RIGHT, along),
                WORLD_TOP - CLOUD_MARGIN_MM + 1.0,
            ),
            1 => (
                lerp(WORLD_LEFT, WORLD_RIGHT, along),
                WORLD_BOT + CLOUD_MARGIN_MM - 1.0,
            ),
            2 => (
                WORLD_LEFT - CLOUD_MARGIN_MM + 1.0,
                lerp(WORLD_TOP, WORLD_BOT, along),
            ),
            _ => (
                WORLD_RIGHT + CLOUD_MARGIN_MM - 1.0,
                lerp(WORLD_TOP, WORLD_BOT, along),
            ),
        };
        let tx = lerp(WORLD_LEFT, WORLD_RIGHT, 0.25 + 0.5 * rng.f32());
        let ty = lerp(WORLD_TOP, WORLD_BOT, 0.25 + 0.5 * rng.f32());
        let (dx, dy) = (tx - x, ty - y);
        let len = (dx * dx + dy * dy).sqrt().max(1.0);
        let speed = rng.range_f32(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX);
        self.x = x;
        self.y = y;
        self.vx = dx / len * speed;
        self.vy = dy / len * speed;
        self.target_vx = self.vx;
        self.target_vy = self.vy;
        self.retarget_start_ms = t_ms;
        self.retarget_len_ms = rng.range_u32(CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
    }
}

struct Strike {
    start_ms: u32,
    // A bolt's path along the tile lattice.
    path: Vec<(f32, f32)>,
    // When each return stroke starts, from start_ms; the first is when the bolt finishes
    // building.
    pulse_ms: Vec<u32>,
}

/// One lightning scheduler: a gap countdown and at most one bolt.
struct StrikeSlot {
    strike: Option<Strike>,
    gap_start_ms: u32,
    gap_len_ms: u32,
}

/// One lit bolt side, with the per-segment terms of the distance test worked out once.
struct Seg {
    a: (f32, f32),
    b: (f32, f32),
    ex: f32,
    ey: f32,
    len2: f32,
    env: f32,
}

impl Seg {
    fn new(a: (f32, f32), b: (f32, f32), env: f32) -> Self {
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        Seg {
            a,
            b,
            ex,
            ey,
            len2: (ex * ex + ey * ey).max(1.0),
            env,
        }
    }

    /// Squared distance from (x, y) to the segment.
    fn dist2(&self, x: f32, y: f32) -> f32 {
        let t = (((x - self.a.0) * self.ex + (y - self.a.1) * self.ey) / self.len2).clamp(0.0, 1.0);
        let (dx, dy) = (x - self.a.0 - self.ex * t, y - self.a.1 - self.ey * t);
        dx * dx + dy * dy
    }
}

/// One upwelling this frame.
struct Bloom {
    x: f32,
    y: f32,
    inv_r2: f32,
    amp: f32,
    ring: f32, // 0 a filled disc, 1 a ring
    reach: usize,
}

/// One soft disc of a cloud this frame.
struct Lobe {
    x: f32,
    y: f32,
    inv_r2: f32,
    reach: usize,
}

/// A night storm seen from above: dark water boiling with upwellings, pale clouds
/// drifting, and sparse lightning.
pub struct Squall {
    clouds: [Cloud; CLOUD_COUNT],
    slots: [StrikeSlot; BOLT_SLOTS],
    lit_sides: Vec<Seg>,
    rng: Rng,
    last_ms: u32,
    seed: u32, // new each time it comes on screen, so the blooms differ
    // The per-LED part of the boil's two inner sines.
    boil_y: [Phasor; LED_COUNT],
    boil_x: [Phasor; LED_COUNT],
    // Per-LED totals for this frame.
    energy: [f32; LED_COUNT],
    density: [f32; LED_COUNT],
    bolt: [f32; LED_COUNT],
}

impl Squall {
    pub fn new() -> Self {
        let mut rng = Rng::new(0x5EED_5EA5);
        let clouds = core::array::from_fn(|_| {
            let x = rng.range_f32(WORLD_LEFT, WORLD_RIGHT);
            let y = rng.range_f32(WORLD_TOP, WORLD_BOT);
            let (vx, vy) = random_velocity(&mut rng);
            Cloud {
                x,
                y,
                vx,
                vy,
                target_vx: vx,
                target_vy: vy,
                retarget_start_ms: 0,
                retarget_len_ms: 0,
                seed: rng.next_u32(),
            }
        });
        let slots = core::array::from_fn(|_| StrikeSlot {
            strike: None,
            gap_start_ms: 0,
            gap_len_ms: rng.range_u32(STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS),
        });
        Squall {
            clouds,
            slots,
            lit_sides: Vec::with_capacity(BOLT_SLOTS * BOLT_SEGS_MAX),
            rng,
            last_ms: 0,
            seed: 0,
            boil_y: core::array::from_fn(|i| {
                Phasor::new(LED_MAP[i].wy * BOIL_ANGLE_PER_MM * BOIL_CROSS_Y)
            }),
            boil_x: core::array::from_fn(|i| {
                Phasor::new(LED_MAP[i].wx * BOIL_ANGLE_PER_MM * BOIL_CROSS_X)
            }),
            energy: [0.0; LED_COUNT],
            density: [0.0; LED_COUNT],
            bolt: [0.0; LED_COUNT],
        }
    }
}

impl Pattern for Squall {
    fn on_enter(&mut self, t_ms: u32) {
        self.seed = random_u32();
        self.last_ms = t_ms;
        // Restart the timers so they do not all fire at once after time off screen.
        for slot in &mut self.slots {
            slot.gap_start_ms = t_ms;
        }
        for c in &mut self.clouds {
            c.retarget_start_ms = t_ms;
        }
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_s = time_step_ms(&mut self.last_ms, t_ms, 100) as f32 / 1000.0;
        self.update_clouds(t_ms, dt_s);
        self.update_strikes(t_ms);

        self.boil(t_ms);
        self.add_blooms(t_ms);
        self.add_clouds(t_ms);
        self.add_lightning(t_ms);

        let foam_phase = Phasor::new(cycle(t_ms, FOAM_PERIOD_MS) * TAU);
        let phasors = phase_phasors();
        for (i, px) in out.iter_mut().enumerate() {
            let energy = self.energy[i];

            // Water color, then twinkling froth above the threshold.
            let mut c = ramp(&WATER_RAMP, energy);
            let excess = ((energy - FOAM_THRESH) * FOAM_GAIN).clamp(0.0, 1.0);
            if excess > 0.0 {
                let step = (knuth_hash(i as u32) % PHASE_STEPS as u32) as usize;
                let tw = phasors[step].sin_plus(foam_phase) * 0.5 + 0.5;
                c = mix_rgb(c, FOAM_COLOR, excess * tw * tw);
            }

            let density = self.density[i].min(1.0);
            c = mix_rgb(c, CLOUD_COLOR, density * CLOUD_BRIGHT);

            let bolt = self.bolt[i];
            if bolt > 0.0 {
                c = mix_rgb(c, FLASH_COLOR, bolt);
            }

            *px = [c[0] as u8, c[1] as u8, c[2] as u8];
        }
    }
}

impl Squall {
    /// Moves the clouds, retargets their headings, and respawns any that have left.
    fn update_clouds(&mut self, t_ms: u32, dt_s: f32) {
        let Squall { clouds, rng, .. } = self;
        let ease = (CLOUD_TURN_RATE * dt_s).min(1.0);
        for c in clouds.iter_mut() {
            if t_ms.wrapping_sub(c.retarget_start_ms) >= c.retarget_len_ms {
                (c.target_vx, c.target_vy) = random_velocity(rng);
                c.retarget_start_ms = t_ms;
                c.retarget_len_ms = rng.range_u32(CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
            }
            c.vx += (c.target_vx - c.vx) * ease;
            c.vy += (c.target_vy - c.vy) * ease;
            c.x += c.vx * dt_s;
            c.y += c.vy * dt_s;
            if c.x < WORLD_LEFT - CLOUD_MARGIN_MM
                || c.x > WORLD_RIGHT + CLOUD_MARGIN_MM
                || c.y < WORLD_TOP - CLOUD_MARGIN_MM
                || c.y > WORLD_BOT + CLOUD_MARGIN_MM
            {
                c.respawn(rng, t_ms);
            }
        }
    }

    /// Per slot: clears a bolt that has faded, and starts the next once its gap is over.
    fn update_strikes(&mut self, t_ms: u32) {
        for i in 0..BOLT_SLOTS {
            let expired = self.slots[i].strike.as_ref().is_some_and(|s| {
                let last = s.pulse_ms.last().copied().unwrap_or(0);
                t_ms.wrapping_sub(s.start_ms) >= last + BOLT_TAIL_MS
            });
            if expired {
                self.slots[i].strike = None;
                self.slots[i].gap_start_ms = t_ms;
                self.slots[i].gap_len_ms = self.rng.range_u32(STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
            }
            if self.slots[i].strike.is_none()
                && t_ms.wrapping_sub(self.slots[i].gap_start_ms) >= self.slots[i].gap_len_ms
            {
                let strike = self.ignite(t_ms);
                self.slots[i].strike = Some(strike);
            }
        }
    }

    /// Starts a bolt at a random cloud and picks its path and return-stroke times.
    fn ignite(&mut self, t_ms: u32) -> Strike {
        let cloud = self.rng.below(CLOUD_COUNT);
        let (cx, cy) = (self.clouds[cloud].x, self.clouds[cloud].y);
        let path = self.walk_bolt(cx, cy);
        let complete = (path.len() as u32 - 1) * BOLT_STEP_MS;
        let pulses = BOLT_PULSES_MIN + self.rng.below(BOLT_PULSES_MAX - BOLT_PULSES_MIN + 1);
        let mut pulse_ms = Vec::with_capacity(pulses);
        let mut at = complete;
        for _ in 0..pulses {
            pulse_ms.push(at);
            at += self.rng.range_u32(PULSE_GAP_MIN_MS, PULSE_GAP_MAX_MS);
        }
        Strike {
            start_ms: t_ms,
            path,
            pulse_ms,
        }
    }

    /// Random walk along the lattice from the vertex nearest (cx, cy). Each hop goes
    /// straight on or turns 60 degrees, never back; the walk ends early if it is cornered.
    fn walk_bolt(&mut self, cx: f32, cy: f32) -> Vec<(f32, f32)> {
        let r0 = (((cy - LATTICE_TOP_Y) / LATTICE_ROW_H).round() as i32).clamp(0, LATTICE_LAST_ROW);
        let last_index = LATTICE_LAST_ROW - r0;
        let i0 = (((cx - WORLD_CX) / LATTICE_SIDE + last_index as f32 / 2.0).round() as i32)
            .clamp(0, last_index);
        let segs = BOLT_SEGS_MIN + self.rng.below(BOLT_SEGS_MAX - BOLT_SEGS_MIN + 1);

        let (mut r, mut i) = (r0, i0);
        let mut here = lattice_pos(r, i);
        let mut pts = Vec::with_capacity(segs + 1);
        pts.push(here);
        let (mut hx, mut hy) = (0.0f32, 0.0f32); // heading of the last hop
        for _ in 0..segs {
            let mut steps = Vec::new();
            for &(dr, di) in &LATTICE_NEIGHBORS {
                let (nr, ni) = (r + dr, i + di);
                if !(0..=LATTICE_LAST_ROW).contains(&nr)
                    || !(0..=LATTICE_LAST_ROW - nr).contains(&ni)
                {
                    continue;
                }
                let (nx, ny) = lattice_pos(nr, ni);
                let (dx, dy) = (nx - here.0, ny - here.1);
                let d = (dx * dx + dy * dy).sqrt();
                if pts.len() == 1 || (hx * dx + hy * dy) / d > 0.0 {
                    steps.push((nr, ni));
                }
            }
            if steps.is_empty() {
                break;
            }
            let (nr, ni) = self.rng.pick(&steps);
            let (nx, ny) = lattice_pos(nr, ni);
            let (dx, dy) = (nx - here.0, ny - here.1);
            let d = (dx * dx + dy * dy).sqrt();
            (hx, hy) = (dx / d, dy / d);
            (r, i) = (nr, ni);
            here = (nx, ny);
            pts.push(here);
        }
        pts
    }

    /// Boiling floor. The inner sines come from the tables; the outer two cannot, since
    /// their arguments contain the inner results.
    fn boil(&mut self, t_ms: u32) {
        let boil1 = cycle(t_ms, BOIL_PERIOD_MS) * TAU;
        let boil2 = cycle(t_ms, BOIL2_PERIOD_MS) * TAU;
        let boil1_phasor = Phasor::new(boil1);
        let boil2_phasor = Phasor::new(boil2);
        for (i, led) in LED_MAP.iter().enumerate() {
            let bx = led.wx * BOIL_ANGLE_PER_MM;
            let by = led.wy * BOIL_ANGLE_PER_MM;
            let mod_y = self.boil_y[i].sin_plus(boil2_phasor);
            let mod_x = self.boil_x[i].sin_plus(boil1_phasor);
            let boil = ((bx + mod_y + boil1).sin() + (by + mod_x + boil2).sin()) * 0.5;
            self.energy[i] = WATER_FLOOR + BOIL_DEPTH * boil;
        }
    }

    /// One bloom per slot per period, at a hashed spot.
    fn blooms(&self, t_ms: u32) -> [Bloom; BLOOM_SLOTS] {
        core::array::from_fn(|slot| {
            let period = BLOOM_PERIOD_MS + slot as u32 * BLOOM_STAGGER_MS;
            let age = cycle(t_ms, period);
            let h = knuth_hash((t_ms / period) ^ self.seed) ^ (slot as u32).wrapping_mul(SLOT_MIX);
            let y = WORLD_TOP + hash01(h, 1) * WORLD_H;
            // Plus a 40 mm margin.
            let half_w = half_width_at(y) + 40.0;
            let x = WORLD_CX + (hash01(h, 2) * 2.0 - 1.0) * half_w;
            let r = lerp(BLOOM_R0_MM, BLOOM_R1_MM, age);
            let reach = (r / CELL_MM).ceil() as usize;
            Bloom {
                x,
                y,
                inv_r2: 1.0 / (r * r),
                amp: BLOOM_PEAK * (age * PI).sin(),
                ring: BLOOM_RIM * age,
                reach,
            }
        })
    }

    /// Upwellings: a young bloom is a filled disc that turns into a ring as it ages.
    fn add_blooms(&mut self, t_ms: u32) {
        for &Bloom {
            x,
            y,
            inv_r2,
            amp,
            ring,
            reach,
        } in &self.blooms(t_ms)
        {
            grid::for_each_near(x, y, reach, |i| {
                let led = &LED_MAP[i];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    let filled = (1.0 - q) * (1.0 - q);
                    let ringed = 4.0 * q * (1.0 - q);
                    self.energy[i] += amp * lerp(filled, ringed, ring);
                }
            });
        }
    }

    /// Every cloud's lobes, each orbiting its cloud center and changing radius.
    fn lobes(&self, t_ms: u32) -> [[Lobe; CLOUD_LOBES]; CLOUD_COUNT] {
        self.clouds.each_ref().map(|c| {
            core::array::from_fn(|j| {
                let h = c.seed ^ (j as u32).wrapping_mul(SLOT_MIX);
                let orbit_p = CLOUD_ORBIT_MIN_MS + h % (CLOUD_ORBIT_MAX_MS - CLOUD_ORBIT_MIN_MS);
                let breathe_p =
                    CLOUD_BREATHE_MIN_MS + (h >> 8) % (CLOUD_BREATHE_MAX_MS - CLOUD_BREATHE_MIN_MS);
                let orbit = (cycle(t_ms, orbit_p) + hash01(h, 1)) * TAU;
                let breathe = (cycle(t_ms, breathe_p) + hash01(h, 2)) * TAU;
                let off = CLOUD_LOBE_OFF_MM * (0.35 + 0.65 * hash01(h, 3));
                let r = CLOUD_LOBE_R_MM * (1.0 + CLOUD_BREATHE * breathe.sin());
                Lobe {
                    x: c.x + orbit.cos() * off,
                    y: c.y + orbit.sin() * off,
                    inv_r2: 1.0 / (r * r),
                    reach: (r / CELL_MM).ceil() as usize,
                }
            })
        })
    }

    /// Cloud cover, densest where lobes overlap.
    fn add_clouds(&mut self, t_ms: u32) {
        self.density.fill(0.0);
        for &Lobe {
            x,
            y,
            inv_r2,
            reach,
        } in self.lobes(t_ms).as_flattened()
        {
            grid::for_each_near(x, y, reach, |i| {
                let led = &LED_MAP[i];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    self.density[i] += (1.0 - q) * (1.0 - q);
                }
            });
        }
    }

    /// The lit sides of every live bolt, each with its brightness.
    fn find_lit_sides(&mut self, t_ms: u32) {
        self.lit_sides.clear();
        for slot in &self.slots {
            let Some(s) = &slot.strike else { continue };
            let elapsed = t_ms.wrapping_sub(s.start_ms);
            if elapsed < s.pulse_ms[0] {
                // Building: every side reached so far is fully lit.
                let reached = (elapsed / BOLT_STEP_MS) as usize + 1;
                for w in s.path[..(reached + 1).min(s.path.len())].windows(2) {
                    self.lit_sides.push(Seg::new(w[0], w[1], 1.0));
                }
            } else {
                // Flickering: each return stroke decays, and the brightest one shows.
                let mut env = 0.0f32;
                for &p in &s.pulse_ms {
                    if elapsed >= p {
                        env = env.max((-((elapsed - p) as f32) / FLICKER_FADE_MS).exp());
                    }
                }
                if env > 0.01 {
                    for w in s.path.windows(2) {
                        self.lit_sides.push(Seg::new(w[0], w[1], env));
                    }
                }
            }
        }
    }

    /// Lightning along the lit tile sides. Where bolts overlap the brighter one wins.
    fn add_lightning(&mut self, t_ms: u32) {
        self.find_lit_sides(t_ms);
        self.bolt.fill(0.0);
        let bolt_reach = (BOLT_REACH_MM / CELL_MM).ceil() as usize;
        for s in &self.lit_sides {
            grid::for_each_near_seg(s.a, s.b, bolt_reach, |i| {
                let led = &LED_MAP[i];
                let d2 = s.dist2(led.wx, led.wy);
                if d2 < BOLT_REACH_MM * BOLT_REACH_MM {
                    let core = (BOLT_GAIN * (-d2 / (BOLT_CORE_MM * BOLT_CORE_MM)).exp()).min(1.0);
                    self.bolt[i] = self.bolt[i].max(s.env * core);
                }
            });
        }
    }
}

/// A cloud velocity in a random direction at a random speed.
fn random_velocity(rng: &mut Rng) -> (f32, f32) {
    let ang = rng.f32() * TAU;
    let speed = rng.range_f32(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX);
    (ang.cos() * speed, ang.sin() * speed)
}

/// World position of lattice vertex (row, index).
fn lattice_pos(r: i32, i: i32) -> (f32, f32) {
    let x = WORLD_CX + (i as f32 - (LATTICE_LAST_ROW - r) as f32 / 2.0) * LATTICE_SIDE;
    (x, LATTICE_TOP_Y + r as f32 * LATTICE_ROW_H)
}
