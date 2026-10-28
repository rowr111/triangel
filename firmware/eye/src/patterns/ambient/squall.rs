use crate::audio::Audio;
use crate::led::geom::{LATTICE_ROW_H, LATTICE_SIDE, LATTICE_TOP_Y};
use crate::led::grid;
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
const BOIL_CELL_MM: f32 = 140.0; // size of one boil cell
const BOIL_ANGLE_PER_MM: f32 = TAU / BOIL_CELL_MM;
// Two unrelated periods, so it boils rather than sweeps.
const BOIL_PERIOD_MS: u32 = 4_900;
const BOIL2_PERIOD_MS: u32 = 3_100;
// Each axis' inner sine, relative to the cell size; not whole numbers, so the two never line up.
const BOIL_INNER_Y: f32 = 0.83;
const BOIL_INNER_X: f32 = 0.71;

// Blooms that swell out of the dark, spread and dissolve.
const BLOOM_SLOTS: usize = 8;
const BLOOM_PERIOD_MS: u32 = 4_600; // base lifetime
const BLOOM_STAGGER_MS: u32 = 733; // added per slot, so they respawn out of step
const BLOOM_START_R_MM: f32 = 45.0; // radius at birth
const BLOOM_END_R_MM: f32 = 175.0; // radius at full spread
const BLOOM_PEAK: f32 = 0.6; // energy a young bloom adds at its center
const BLOOM_RIM: f32 = 0.55; // 0 fades in place, 1 dies as an expanding ring

// Foam: twinkling white where the energy passes the threshold.
const FOAM_THRESHOLD: f32 = 0.8;
const FOAM_GAIN: f32 = 2.5; // how fast foam saturates past the threshold
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
const CLOUD_LOBE_ORBIT_MM: f32 = 60.0; // lobe orbit radius around the cloud center
const CLOUD_BREATHE: f32 = 0.3; // lobe radius swing
const CLOUD_ORBIT_MIN_MS: u32 = 9_000; // time for a lobe to circle its cloud
const CLOUD_ORBIT_MAX_MS: u32 = 16_000;
const CLOUD_BREATHE_MIN_MS: u32 = 3_500; // time for a lobe to swell and shrink
const CLOUD_BREATHE_MAX_MS: u32 = 6_500;
const CLOUD_OPACITY: f32 = 0.7; // opacity at full density
const CLOUD_MARGIN_MM: f32 = 170.0; // how far past an edge before re-entering

// Lightning: a white bolt builds along the tile edges one side at a time, flickers
// through a few pulses, then dies.
const BOLT_SLOTS: usize = 2; // bolts that can be alive at once
const STRIKE_GAP_MIN_MS: u32 = 4_000; // per slot
const STRIKE_GAP_MAX_MS: u32 = 14_000;
const BOLT_SIDES_MIN: usize = 3; // bolt length, in tile sides
const BOLT_SIDES_MAX: usize = 8;
const BOLT_STEP_MS: u32 = 150; // time per tile side while building
const PULSE_FADE_MS: f32 = 180.0; // decay of each pulse
const BOLT_PULSES_MIN: usize = 2; // pulses per strike
const BOLT_PULSES_MAX: usize = 5;
const PULSE_GAP_MIN_MS: u32 = 60; // between pulses
const PULSE_GAP_MAX_MS: u32 = 420;
const BOLT_TAIL_MS: u32 = 1_400; // slot held after the last pulse; must cover the fade
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
        let aim_x = lerp(WORLD_LEFT, WORLD_RIGHT, 0.25 + 0.5 * rng.f32());
        let aim_y = lerp(WORLD_TOP, WORLD_BOT, 0.25 + 0.5 * rng.f32());
        let (dx, dy) = (aim_x - x, aim_y - y);
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
    // When each pulse starts, from start_ms; the first is when the bolt finishes building.
    pulse_ms: Vec<u32>,
}

/// One lightning scheduler: a gap countdown and at most one bolt.
struct StrikeSlot {
    strike: Option<Strike>,
    gap_start_ms: u32,
    gap_len_ms: u32,
}

/// One lit bolt side, with the terms of the distance test worked out once.
struct LitSide {
    start: (f32, f32),
    end: (f32, f32),
    dx: f32,
    dy: f32,
    len2: f32,
    brightness: f32,
}

impl LitSide {
    fn new(start: (f32, f32), end: (f32, f32), brightness: f32) -> Self {
        let (dx, dy) = (end.0 - start.0, end.1 - start.1);
        LitSide {
            start,
            end,
            dx,
            dy,
            len2: (dx * dx + dy * dy).max(1.0),
            brightness,
        }
    }

    /// Squared distance from (x, y) to the side.
    fn dist2(&self, x: f32, y: f32) -> f32 {
        let t = (((x - self.start.0) * self.dx + (y - self.start.1) * self.dy) / self.len2)
            .clamp(0.0, 1.0);
        let (gap_x, gap_y) = (
            x - self.start.0 - self.dx * t,
            y - self.start.1 - self.dy * t,
        );
        gap_x * gap_x + gap_y * gap_y
    }
}

/// One bloom this frame.
struct Bloom {
    x: f32,
    y: f32,
    inv_r2: f32,
    amp: f32,
    ring: f32, // 0 a filled disc, 1 a ring
    radius: f32,
}

/// One soft disc of a cloud this frame.
struct Lobe {
    x: f32,
    y: f32,
    inv_r2: f32,
    radius: f32,
}

/// A night storm seen from above: dark water boiling with blooms, pale clouds drifting,
/// and sparse lightning.
pub struct Squall {
    clouds: [Cloud; CLOUD_COUNT],
    slots: [StrikeSlot; BOLT_SLOTS],
    lit_sides: Vec<LitSide>,
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
            lit_sides: Vec::with_capacity(BOLT_SLOTS * BOLT_SIDES_MAX),
            rng,
            last_ms: 0,
            seed: 0,
            boil_y: core::array::from_fn(|i| {
                Phasor::new(LED_MAP[i].wy * BOIL_ANGLE_PER_MM * BOIL_INNER_Y)
            }),
            boil_x: core::array::from_fn(|i| {
                Phasor::new(LED_MAP[i].wx * BOIL_ANGLE_PER_MM * BOIL_INNER_X)
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
        for cloud in &mut self.clouds {
            cloud.retarget_start_ms = t_ms;
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

            // Water color, then twinkling foam above the threshold.
            let mut color = ramp(&WATER_RAMP, energy);
            let excess = ((energy - FOAM_THRESHOLD) * FOAM_GAIN).clamp(0.0, 1.0);
            if excess > 0.0 {
                let step = (knuth_hash(i as u32) % PHASE_STEPS as u32) as usize;
                let twinkle = phasors[step].sin_plus(foam_phase) * 0.5 + 0.5;
                color = mix_rgb(color, FOAM_COLOR, excess * twinkle * twinkle);
            }

            let density = self.density[i].min(1.0);
            color = mix_rgb(color, CLOUD_COLOR, density * CLOUD_OPACITY);

            let bolt = self.bolt[i];
            if bolt > 0.0 {
                color = mix_rgb(color, FLASH_COLOR, bolt);
            }

            *px = color.map(|v| v as u8);
        }
    }
}

impl Squall {
    /// Moves the clouds, retargets their headings, and respawns any that have left.
    fn update_clouds(&mut self, t_ms: u32, dt_s: f32) {
        let Squall { clouds, rng, .. } = self;
        let ease = (CLOUD_TURN_RATE * dt_s).min(1.0);
        for cloud in clouds.iter_mut() {
            if t_ms.wrapping_sub(cloud.retarget_start_ms) >= cloud.retarget_len_ms {
                (cloud.target_vx, cloud.target_vy) = random_velocity(rng);
                cloud.retarget_start_ms = t_ms;
                cloud.retarget_len_ms = rng.range_u32(CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
            }
            cloud.vx += (cloud.target_vx - cloud.vx) * ease;
            cloud.vy += (cloud.target_vy - cloud.vy) * ease;
            cloud.x += cloud.vx * dt_s;
            cloud.y += cloud.vy * dt_s;
            if cloud.x < WORLD_LEFT - CLOUD_MARGIN_MM
                || cloud.x > WORLD_RIGHT + CLOUD_MARGIN_MM
                || cloud.y < WORLD_TOP - CLOUD_MARGIN_MM
                || cloud.y > WORLD_BOT + CLOUD_MARGIN_MM
            {
                cloud.respawn(rng, t_ms);
            }
        }
    }

    /// Per slot: clears a bolt that has faded, and starts the next once its gap is over.
    fn update_strikes(&mut self, t_ms: u32) {
        for i in 0..BOLT_SLOTS {
            let expired = self.slots[i].strike.as_ref().is_some_and(|strike| {
                let last = strike.pulse_ms.last().copied().unwrap_or(0);
                t_ms.wrapping_sub(strike.start_ms) >= last + BOLT_TAIL_MS
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

    /// Starts a bolt at a random cloud and picks its path and pulse times.
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
    /// straight on or turns 60 degrees, never back; the walk stops early when no hop is possible.
    fn walk_bolt(&mut self, cx: f32, cy: f32) -> Vec<(f32, f32)> {
        let start_row =
            (((cy - LATTICE_TOP_Y) / LATTICE_ROW_H).round() as i32).clamp(0, LATTICE_LAST_ROW);
        let last_index = LATTICE_LAST_ROW - start_row;
        let start_index = (((cx - WORLD_CX) / LATTICE_SIDE + last_index as f32 / 2.0).round()
            as i32)
            .clamp(0, last_index);
        let sides = BOLT_SIDES_MIN + self.rng.below(BOLT_SIDES_MAX - BOLT_SIDES_MIN + 1);

        let (mut row, mut index) = (start_row, start_index);
        let mut here = lattice_pos(row, index);
        let mut path = Vec::with_capacity(sides + 1);
        path.push(here);
        let (mut heading_x, mut heading_y) = (0.0f32, 0.0f32); // heading of the last hop
        for _ in 0..sides {
            let mut choices = Vec::new();
            for &(row_step, index_step) in &LATTICE_NEIGHBORS {
                let (next_row, next_index) = (row + row_step, index + index_step);
                if !(0..=LATTICE_LAST_ROW).contains(&next_row)
                    || !(0..=LATTICE_LAST_ROW - next_row).contains(&next_index)
                {
                    continue;
                }
                let (next_x, next_y) = lattice_pos(next_row, next_index);
                let (dx, dy) = (next_x - here.0, next_y - here.1);
                let len = (dx * dx + dy * dy).sqrt();
                if path.len() == 1 || (heading_x * dx + heading_y * dy) / len > 0.0 {
                    choices.push((next_row, next_index));
                }
            }
            if choices.is_empty() {
                break;
            }
            let (next_row, next_index) = self.rng.pick(&choices);
            let (next_x, next_y) = lattice_pos(next_row, next_index);
            let (dx, dy) = (next_x - here.0, next_y - here.1);
            let len = (dx * dx + dy * dy).sqrt();
            (heading_x, heading_y) = (dx / len, dy / len);
            (row, index) = (next_row, next_index);
            here = (next_x, next_y);
            path.push(here);
        }
        path
    }

    /// Boiling floor. Only the inner sines can come from the per-LED tables.
    fn boil(&mut self, t_ms: u32) {
        let boil1 = cycle(t_ms, BOIL_PERIOD_MS) * TAU;
        let boil2 = cycle(t_ms, BOIL2_PERIOD_MS) * TAU;
        let boil1_phasor = Phasor::new(boil1);
        let boil2_phasor = Phasor::new(boil2);
        for (i, led) in LED_MAP.iter().enumerate() {
            let angle_x = led.wx * BOIL_ANGLE_PER_MM;
            let angle_y = led.wy * BOIL_ANGLE_PER_MM;
            let inner_y = self.boil_y[i].sin_plus(boil2_phasor);
            let inner_x = self.boil_x[i].sin_plus(boil1_phasor);
            let boil =
                ((angle_x + inner_y + boil1).sin() + (angle_y + inner_x + boil2).sin()) * 0.5;
            self.energy[i] = WATER_FLOOR + BOIL_DEPTH * boil;
        }
    }

    /// One bloom per slot per period, at a hashed spot.
    fn blooms(&self, t_ms: u32) -> [Bloom; BLOOM_SLOTS] {
        core::array::from_fn(|slot| {
            let period = BLOOM_PERIOD_MS + slot as u32 * BLOOM_STAGGER_MS;
            let age = cycle(t_ms, period);
            let hash =
                knuth_hash((t_ms / period) ^ self.seed) ^ (slot as u32).wrapping_mul(SLOT_MIX);
            let y = WORLD_TOP + hash01(hash, 1) * WORLD_H;
            // Plus a 40 mm margin.
            let half_width = half_width_at(y) + 40.0;
            let x = WORLD_CX + (hash01(hash, 2) * 2.0 - 1.0) * half_width;
            let radius = lerp(BLOOM_START_R_MM, BLOOM_END_R_MM, age);
            Bloom {
                x,
                y,
                inv_r2: 1.0 / (radius * radius),
                amp: BLOOM_PEAK * (age * PI).sin(),
                ring: BLOOM_RIM * age,
                radius,
            }
        })
    }

    /// A young bloom is a filled disc that turns into a ring as it ages.
    fn add_blooms(&mut self, t_ms: u32) {
        for &Bloom {
            x,
            y,
            inv_r2,
            amp,
            ring,
            radius,
        } in &self.blooms(t_ms)
        {
            grid::for_each_near(x, y, radius, |i| {
                let led = &LED_MAP[i];
                let d2_ratio = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if d2_ratio < 1.0 {
                    let filled = (1.0 - d2_ratio) * (1.0 - d2_ratio);
                    let ringed = 4.0 * d2_ratio * (1.0 - d2_ratio);
                    self.energy[i] += amp * lerp(filled, ringed, ring);
                }
            });
        }
    }

    /// Every cloud's lobes, each orbiting its cloud center and changing radius.
    fn lobes(&self, t_ms: u32) -> [[Lobe; CLOUD_LOBES]; CLOUD_COUNT] {
        self.clouds.each_ref().map(|cloud| {
            core::array::from_fn(|j| {
                let hash = cloud.seed ^ (j as u32).wrapping_mul(SLOT_MIX);
                let orbit_ms =
                    CLOUD_ORBIT_MIN_MS + hash % (CLOUD_ORBIT_MAX_MS - CLOUD_ORBIT_MIN_MS);
                let breathe_ms = CLOUD_BREATHE_MIN_MS
                    + (hash >> 8) % (CLOUD_BREATHE_MAX_MS - CLOUD_BREATHE_MIN_MS);
                let orbit = (cycle(t_ms, orbit_ms) + hash01(hash, 1)) * TAU;
                let breathe = (cycle(t_ms, breathe_ms) + hash01(hash, 2)) * TAU;
                let orbit_mm = CLOUD_LOBE_ORBIT_MM * (0.35 + 0.65 * hash01(hash, 3));
                let radius = CLOUD_LOBE_R_MM * (1.0 + CLOUD_BREATHE * breathe.sin());
                Lobe {
                    x: cloud.x + orbit.cos() * orbit_mm,
                    y: cloud.y + orbit.sin() * orbit_mm,
                    inv_r2: 1.0 / (radius * radius),
                    radius,
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
            radius,
        } in self.lobes(t_ms).as_flattened()
        {
            grid::for_each_near(x, y, radius, |i| {
                let led = &LED_MAP[i];
                let d2_ratio = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if d2_ratio < 1.0 {
                    self.density[i] += (1.0 - d2_ratio) * (1.0 - d2_ratio);
                }
            });
        }
    }

    /// The lit sides of every live bolt, each with its brightness.
    fn find_lit_sides(&mut self, t_ms: u32) {
        self.lit_sides.clear();
        for slot in &self.slots {
            let Some(strike) = &slot.strike else { continue };
            let elapsed = t_ms.wrapping_sub(strike.start_ms);
            if elapsed < strike.pulse_ms[0] {
                // Building: every side reached so far is fully lit.
                let reached = (elapsed / BOLT_STEP_MS) as usize + 1;
                for pair in strike.path[..(reached + 1).min(strike.path.len())].windows(2) {
                    self.lit_sides.push(LitSide::new(pair[0], pair[1], 1.0));
                }
            } else {
                // Flickering: each pulse decays, and the brightest one shows.
                let mut brightness = 0.0f32;
                for &pulse_start in &strike.pulse_ms {
                    if elapsed >= pulse_start {
                        let since = (elapsed - pulse_start) as f32;
                        brightness = brightness.max((-since / PULSE_FADE_MS).exp());
                    }
                }
                if brightness > 0.01 {
                    for pair in strike.path.windows(2) {
                        self.lit_sides
                            .push(LitSide::new(pair[0], pair[1], brightness));
                    }
                }
            }
        }
    }

    /// Lightning along the lit tile sides. Where bolts overlap the brighter one wins.
    fn add_lightning(&mut self, t_ms: u32) {
        self.find_lit_sides(t_ms);
        self.bolt.fill(0.0);
        for side in &self.lit_sides {
            grid::for_each_near_seg(side.start, side.end, BOLT_REACH_MM, |i| {
                let led = &LED_MAP[i];
                let d2 = side.dist2(led.wx, led.wy);
                if d2 < BOLT_REACH_MM * BOLT_REACH_MM {
                    let core = (BOLT_GAIN * (-d2 / (BOLT_CORE_MM * BOLT_CORE_MM)).exp()).min(1.0);
                    self.bolt[i] = self.bolt[i].max(side.brightness * core);
                }
            });
        }
    }
}

/// A cloud velocity in a random direction at a random speed.
fn random_velocity(rng: &mut Rng) -> (f32, f32) {
    let angle = rng.f32() * TAU;
    let speed = rng.range_f32(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX);
    (angle.cos() * speed, angle.sin() * speed)
}

/// World position of lattice vertex (row, index).
fn lattice_pos(row: i32, index: i32) -> (f32, f32) {
    let x = WORLD_CX + (index as f32 - (LATTICE_LAST_ROW - row) as f32 / 2.0) * LATTICE_SIDE;
    (x, LATTICE_TOP_Y + row as f32 * LATTICE_ROW_H)
}
