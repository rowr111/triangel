use crate::patterns::{Frame, Pattern, Rng, cycle, knuth_hash, lerp, mix_rgb, phase_phasors, ramp, PHASE_STEPS};
use crate::led::grid::{self, CELL_MM};
use crate::audio::Audio;
use crate::led::map::{WORLD_BOT, WORLD_CX, WORLD_H, WORLD_TOP, LED_COUNT, LED_MAP};
use crate::led::world::{WORLD_HALF_W, WORLD_LEFT, WORLD_RIGHT};
use core::f32::consts::{PI, TAU};

// Water floor: the resting level, plus a slow boil in place.
const WATER_FLOOR: f32 = 0.40;
const BOIL_DEPTH:  f32 = 0.10;
const BOIL_CELL_MM: f32 = 140.0;    // size of the churn
const BOIL_PERIOD_MS:  u32 = 4_900; // two unrelated periods, so it boils
const BOIL2_PERIOD_MS: u32 = 3_100; // rather than sweeps
const BOIL_CROSS_Y: f32 = 0.83; // each axis' modulating sine, relative to the cell size;
const BOIL_CROSS_X: f32 = 0.71; // off-integer so the two never line up

// Upwellings: blooms that swell out of the dark, spread and dissolve.
const BLOOM_SLOTS: usize = 8;
const BLOOM_PERIOD_MS:  u32 = 4_600; // base lifetime
const BLOOM_STAGGER_MS: u32 = 733;   // added per slot, so they respawn out of step
const BLOOM_R0_MM: f32 = 45.0;  // radius at birth
const BLOOM_R1_MM: f32 = 175.0; // radius at full spread
const BLOOM_PEAK:  f32 = 0.6;   // energy a young bloom adds at its center
const BLOOM_RIM:   f32 = 0.55;  // 0 fades in place, 1 dies as an expanding ring

// Whitecaps: twinkling froth where the energy passes the threshold.
const FOAM_THRESH: f32 = 0.8;
const FOAM_GAIN:   f32 = 2.5;      // how fast froth saturates past the threshold
const FOAM_PERIOD_MS: u32 = 530;   // twinkle period

// Clouds: pale masses that drift, turn toward a new heading now and then, and come back
// in from another edge after leaving.
const CLOUD_COUNT: usize = 3;
const CLOUD_LOBES: usize = 3;        // overlapping soft lobes per cloud
const CLOUD_SPEED_MIN: f32 = 12.0;   // mm/s
const CLOUD_SPEED_MAX: f32 = 40.0;
const CLOUD_TURN_RATE: f32 = 0.8;    // 1/s, how fast velocity eases to its target
const CLOUD_RETARGET_MIN_MS: u32 = 3_000; // how long a heading holds
const CLOUD_RETARGET_MAX_MS: u32 = 9_000;
const CLOUD_LOBE_R_MM:   f32 = 90.0;
const CLOUD_LOBE_OFF_MM: f32 = 60.0; // lobe orbit radius around the cloud center
const CLOUD_BREATHE: f32 = 0.3;      // lobe radius swing
const CLOUD_BRIGHT:  f32 = 0.7;      // opacity at full density
const CLOUD_MARGIN_MM: f32 = 170.0;  // how far past an edge before re-entering

// Lightning: a white bolt builds along the tile edges one side at a time, flickers
// through a few return strokes, then dies.
const BOLT_SLOTS: usize = 2;         // bolts that can be alive at once
const STRIKE_GAP_MIN_MS: u32 = 4_000;  // per slot
const STRIKE_GAP_MAX_MS: u32 = 14_000;
const BOLT_SEGS_MIN: usize = 3;      // bolt length, in tile sides
const BOLT_SEGS_MAX: usize = 8;
const BOLT_STEP_MS: u32 = 150;       // time per tile side while building
const FLICKER_FADE_MS: f32 = 180.0;  // decay of each return stroke
const BOLT_PULSES_MIN: usize = 2;    // return strokes per strike
const BOLT_PULSES_MAX: usize = 5;
const BOLT_TAIL_MS: u32 = 1_400;     // fade-out time after the last stroke
const BOLT_CORE_MM: f32 = 12.0;      // reaches the two LED rows either side of a tile edge
const BOLT_GAIN: f32 = 1.6;          // LEDs near the line saturate to full white

// Tile-corner lattice the bolts walk: row r (0 = top edge, 5 = apex) holds 6-r vertices
// centered on WORLD_CX. Side and row height include the gap between tiles.
const LATTICE_TOP_Y: f32 = 1.0;
const LATTICE_ROW_H: f32 = 89.0;
const LATTICE_SIDE:  f32 = 103.5;
// Neighbors as (row, index) steps: left, right, and the four diagonals.
const LATTICE_NEIGHBORS: [(i32, i32); 6] = [(0, -1), (0, 1), (-1, 0), (-1, 1), (1, -1), (1, 0)];

const FOAM_COLOR:  [f32; 3] = [235.0, 245.0, 250.0];
const CLOUD_COLOR: [f32; 3] = [168.0, 178.0, 196.0]; // pale blue-gray
const FLASH_COLOR: [f32; 3] = [255.0, 255.0, 255.0];

struct Cloud {
    x:   f32,
    y:   f32,
    vx:  f32,
    vy:  f32,
    tvx: f32, // target velocity
    tvy: f32,
    retarget_start_ms: u32,
    retarget_len_ms:   u32,
    seed: u32, // makes this cloud's lobes move differently from the others'
}

/// A bolt's path along the tile lattice.
struct Bolt {
    pts: [(f32, f32); BOLT_SEGS_MAX + 1],
    len: usize, // points in use
}

struct Strike {
    start_ms: u32,
    bolt:     Bolt,
    // When each return stroke starts, from start_ms; the first is when the bolt finishes
    // building. u32::MAX marks an unused slot.
    pulse_ms: [u32; BOLT_PULSES_MAX],
}

/// One lightning scheduler: a gap countdown and at most one bolt.
struct StrikeSlot {
    strike:       Option<Strike>,
    gap_start_ms: u32,
    gap_len_ms:   u32,
}

/// A night storm seen from above: dark water boiling with upwellings, pale clouds
/// drifting, and sparse lightning.
pub struct Squall {
    clouds:  [Cloud; CLOUD_COUNT],
    slots:   [StrikeSlot; BOLT_SLOTS],
    rng:     Rng,
    last_ms: Option<u32>, // None until the first render
    // sin and cos of the per-LED part of the boil's two inner sines.
    by_sin: [f32; LED_COUNT],
    by_cos: [f32; LED_COUNT],
    bx_sin: [f32; LED_COUNT],
    bx_cos: [f32; LED_COUNT],
    // Per-LED totals for this frame.
    energy:  [f32; LED_COUNT],
    density: [f32; LED_COUNT],
    bolt:    [f32; LED_COUNT],
}

impl Squall {
    pub fn new() -> Self {
        let mut rng = Rng::new(0x5EED_5EA5);
        let clouds = core::array::from_fn(|_| {
            let x = lerp(WORLD_LEFT, WORLD_RIGHT, rng.f32());
            let y = lerp(WORLD_TOP, WORLD_BOT, rng.f32());
            let ang = rng.f32() * TAU;
            let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rng.f32());
            let (vx, vy) = (ang.cos() * speed, ang.sin() * speed);
            Cloud {
                x, y, vx, vy, tvx: vx, tvy: vy,
                retarget_start_ms: 0,
                retarget_len_ms:   0,
                seed: rng.next_u32(),
            }
        });
        let slots = core::array::from_fn(|_| StrikeSlot {
            strike:       None,
            gap_start_ms: 0,
            gap_len_ms:   0,
        });
        let k = TAU / BOIL_CELL_MM;
        Squall {
            clouds,
            slots,
            rng,
            last_ms: None,
            by_sin: core::array::from_fn(|i| (LED_MAP[i].wy * k * BOIL_CROSS_Y).sin()),
            by_cos: core::array::from_fn(|i| (LED_MAP[i].wy * k * BOIL_CROSS_Y).cos()),
            bx_sin: core::array::from_fn(|i| (LED_MAP[i].wx * k * BOIL_CROSS_X).sin()),
            bx_cos: core::array::from_fn(|i| (LED_MAP[i].wx * k * BOIL_CROSS_X).cos()),
            energy:  [0.0; LED_COUNT],
            density: [0.0; LED_COUNT],
            bolt:    [0.0; LED_COUNT],
        }
    }

    /// Moves the clouds, retargets their headings, and respawns any that have left.
    fn update_clouds(&mut self, t_ms: u32, dt_s: f32) {
        let Squall { clouds, rng, .. } = self;
        let ease = (CLOUD_TURN_RATE * dt_s).min(1.0);
        for c in clouds.iter_mut() {
            if t_ms.wrapping_sub(c.retarget_start_ms) >= c.retarget_len_ms {
                let ang = rng.f32() * TAU;
                let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rng.f32());
                c.tvx = ang.cos() * speed;
                c.tvy = ang.sin() * speed;
                c.retarget_start_ms = t_ms;
                c.retarget_len_ms = rng.range_u32(CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
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

    /// Per slot: clears a bolt that has faded, and starts the next once its gap is over.
    fn update_strikes(&mut self, t_ms: u32) {
        for k in 0..BOLT_SLOTS {
            let expired = self.slots[k].strike.as_ref().is_some_and(|s| {
                let last = s.pulse_ms.iter().rev().copied().find(|&p| p != u32::MAX).unwrap_or(0);
                t_ms.wrapping_sub(s.start_ms) >= last + BOLT_TAIL_MS
            });
            if expired {
                self.slots[k].strike = None;
                self.slots[k].gap_start_ms = t_ms;
                self.slots[k].gap_len_ms =
                    self.rng.range_u32(STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
            }
            if self.slots[k].strike.is_none()
                && t_ms.wrapping_sub(self.slots[k].gap_start_ms) >= self.slots[k].gap_len_ms
            {
                let strike = self.ignite(t_ms);
                self.slots[k].strike = Some(strike);
            }
        }
    }

    /// Starts a bolt at a random cloud and picks its path and return-stroke times.
    fn ignite(&mut self, t_ms: u32) -> Strike {
        let a = self.rng.next_u32() as usize % CLOUD_COUNT;
        let (cx, cy) = (self.clouds[a].x, self.clouds[a].y);
        let bolt = self.walk_bolt(cx, cy);
        let complete = (bolt.len as u32 - 1) * BOLT_STEP_MS;
        let n = BOLT_PULSES_MIN
            + self.rng.next_u32() as usize % (BOLT_PULSES_MAX - BOLT_PULSES_MIN + 1);
        let mut pulse_ms = [u32::MAX; BOLT_PULSES_MAX];
        let mut at = complete;
        for p in pulse_ms.iter_mut().take(n) {
            *p = at;
            at += 60 + self.rng.next_u32() % 360;
        }
        Strike { start_ms: t_ms, bolt, pulse_ms }
    }

    /// Random walk along the lattice from the vertex nearest (cx, cy). Each hop goes
    /// straight on or turns 60 degrees, never back; the walk ends early if it is cornered.
    fn walk_bolt(&mut self, cx: f32, cy: f32) -> Bolt {
        let r0 = (((cy - LATTICE_TOP_Y) / LATTICE_ROW_H).round() as i32).clamp(0, 5);
        let i0 = (((cx - WORLD_CX) / LATTICE_SIDE + (5 - r0) as f32 / 2.0).round() as i32)
            .clamp(0, 5 - r0);
        let segs = BOLT_SEGS_MIN
            + self.rng.next_u32() as usize % (BOLT_SEGS_MAX - BOLT_SEGS_MIN + 1);

        let (mut r, mut i) = (r0, i0);
        let mut pts = [(0.0f32, 0.0f32); BOLT_SEGS_MAX + 1];
        pts[0] = lattice_pos(r, i);
        let mut len = 1;
        let (mut hx, mut hy) = (0.0f32, 0.0f32); // heading of the last hop
        for _ in 0..segs {
            let mut cand = [(0i32, 0i32); 6];
            let mut n = 0;
            for &(dr, di) in &LATTICE_NEIGHBORS {
                let (nr, ni) = (r + dr, i + di);
                if !(0..=5).contains(&nr) || !(0..=5 - nr).contains(&ni) {
                    continue;
                }
                let (nx, ny) = lattice_pos(nr, ni);
                let (dx, dy) = (nx - pts[len - 1].0, ny - pts[len - 1].1);
                let d = (dx * dx + dy * dy).sqrt();
                if len == 1 || (hx * dx + hy * dy) / d > 0.0 {
                    cand[n] = (nr, ni);
                    n += 1;
                }
            }
            if n == 0 {
                break;
            }
            let (nr, ni) = cand[self.rng.next_u32() as usize % n];
            let (nx, ny) = lattice_pos(nr, ni);
            let (dx, dy) = (nx - pts[len - 1].0, ny - pts[len - 1].1);
            let d = (dx * dx + dy * dy).sqrt();
            (hx, hy) = (dx / d, dy / d);
            (r, i) = (nr, ni);
            pts[len] = (nx, ny);
            len += 1;
        }
        Bolt { pts, len }
    }
}

impl Pattern for Squall {
    fn on_enter(&mut self, t_ms: u32) {
        // Restart the timers so they do not all fire at once after time off screen.
        for slot in &mut self.slots {
            slot.gap_start_ms = t_ms;
        }
        for c in &mut self.clouds {
            c.retarget_start_ms = t_ms;
        }
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let boil1 = cycle(t_ms, BOIL_PERIOD_MS) * TAU;
        let boil2 = cycle(t_ms, BOIL2_PERIOD_MS) * TAU;
        let foam_phase = cycle(t_ms, FOAM_PERIOD_MS) * TAU;

        let dt_s = match self.last_ms {
            Some(last) => (t_ms.wrapping_sub(last) as f32 / 1000.0).min(0.1),
            None => {
                // First frame: start every gap now, so no bolt fires straight away.
                let Squall { slots, rng, .. } = self;
                for slot in slots.iter_mut() {
                    slot.gap_start_ms = t_ms;
                    slot.gap_len_ms = rng.range_u32(STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
                }
                0.0
            }
        };
        self.last_ms = Some(t_ms);
        self.update_clouds(t_ms, dt_s);
        self.update_strikes(t_ms);

        // One bloom per slot per period, at a hashed spot: (x, y, 1/r^2, amplitude, ring
        // blend, grid reach).
        let mut blooms = [(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0usize); BLOOM_SLOTS];
        for (k, b) in blooms.iter_mut().enumerate() {
            let period = BLOOM_PERIOD_MS + k as u32 * BLOOM_STAGGER_MS;
            let p = cycle(t_ms, period);
            let h = knuth_hash(t_ms / period) ^ (k as u32).wrapping_mul(0x9E37_79B9);
            let y = WORLD_TOP + ((h >> 10) % 1000) as f32 / 1000.0 * WORLD_H;
            let hw = (WORLD_BOT - y) / WORLD_H * WORLD_HALF_W + 40.0; // triangle width at y
            let x = WORLD_CX + ((h % 1000) as f32 / 1000.0 * 2.0 - 1.0) * hw;
            let r = lerp(BLOOM_R0_MM, BLOOM_R1_MM, p);
            let reach = (r / CELL_MM).ceil() as usize;
            *b = (x, y, 1.0 / (r * r), BLOOM_PEAK * (p * PI).sin(), BLOOM_RIM * p, reach);
        }

        // Every cloud's lobes, each orbiting its cloud center and changing radius:
        // (x, y, 1/r^2, grid reach).
        let mut lobes = [(0.0f32, 0.0f32, 0.0f32, 0usize); CLOUD_COUNT * CLOUD_LOBES];
        for (ci, c) in self.clouds.iter().enumerate() {
            for j in 0..CLOUD_LOBES {
                let h = c.seed ^ (j as u32).wrapping_mul(0x9E37_79B9);
                let orbit_p = 9_000 + h % 7_000;
                let breathe_p = 3_500 + (h >> 8) % 3_000;
                let orbit = cycle(t_ms, orbit_p) * TAU + (h >> 16) as f32;
                let breathe = cycle(t_ms, breathe_p) * TAU + (h >> 12) as f32;
                let off = CLOUD_LOBE_OFF_MM * (0.35 + 0.65 * ((h >> 4) % 100) as f32 / 100.0);
                let r = CLOUD_LOBE_R_MM * (1.0 + CLOUD_BREATHE * breathe.sin());
                lobes[ci * CLOUD_LOBES + j] = (
                    c.x + orbit.cos() * off,
                    c.y + orbit.sin() * off,
                    1.0 / (r * r),
                    (r / CELL_MM).ceil() as usize,
                );
            }
        }

        // The lit sides of every live bolt, each with its brightness.
        let mut segs = [Seg::ZERO; BOLT_SLOTS * BOLT_SEGS_MAX];
        let mut n_segs = 0;
        for slot in &self.slots {
            let Some(s) = &slot.strike else { continue };
            let el = t_ms.wrapping_sub(s.start_ms);
            if el < s.pulse_ms[0] {
                // Building: every side reached so far is fully lit.
                let reached = (el / BOLT_STEP_MS) as usize + 1;
                for w in s.bolt.pts[..(reached + 1).min(s.bolt.len)].windows(2) {
                    segs[n_segs] = Seg::new(w[0], w[1], 1.0);
                    n_segs += 1;
                }
            } else {
                // Flickering: each return stroke decays, and the brightest one shows.
                let mut env = 0.0f32;
                for &p in &s.pulse_ms {
                    if el >= p {
                        env = env.max((-((el - p) as f32) / FLICKER_FADE_MS).exp());
                    }
                }
                if env > 0.01 {
                    for w in s.bolt.pts[..s.bolt.len].windows(2) {
                        segs[n_segs] = Seg::new(w[0], w[1], env);
                        n_segs += 1;
                    }
                }
            }
        }
        let segs = &segs[..n_segs];

        let boil_k = TAU / BOIL_CELL_MM;
        let (b1_sin, b1_cos) = boil1.sin_cos();
        let (b2_sin, b2_cos) = boil2.sin_cos();
        let (fp_sin, fp_cos) = foam_phase.sin_cos();
        let (ph_sin, ph_cos) = phase_phasors();

        // Boiling floor. The inner sines come from the tables; the outer two cannot, since
        // their arguments contain the inner results.
        let Squall {
            by_sin,
            by_cos,
            bx_sin,
            bx_cos,
            energy: energy_buf,
            density: density_buf,
            bolt: bolt_buf,
            ..
        } = self;
        for (i, led) in LED_MAP.iter().enumerate() {
            let bx = led.wx * boil_k;
            let by = led.wy * boil_k;
            let mod_y = by_sin[i] * b2_cos + by_cos[i] * b2_sin;
            let mod_x = bx_sin[i] * b1_cos + bx_cos[i] * b1_sin;
            let boil = ((bx + mod_y + boil1).sin() + (by + mod_x + boil2).sin()) * 0.5;
            energy_buf[i] = WATER_FLOOR + BOIL_DEPTH * boil;
            density_buf[i] = 0.0;
            bolt_buf[i] = 0.0;
        }

        // Upwellings: a young bloom is a filled disc that turns into a ring as it ages.
        for &(x, y, inv_r2, amp, ring, reach) in &blooms {
            grid::for_each_near(x, y, reach, |k| {
                let led = &LED_MAP[k];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    let filled = (1.0 - q) * (1.0 - q);
                    let ringed = 4.0 * q * (1.0 - q);
                    energy_buf[k] += amp * lerp(filled, ringed, ring);
                }
            });
        }

        // Cloud cover, densest where lobes overlap.
        for &(x, y, inv_r2, reach) in &lobes {
            grid::for_each_near(x, y, reach, |k| {
                let led = &LED_MAP[k];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    density_buf[k] += (1.0 - q) * (1.0 - q);
                }
            });
        }

        // Lightning along the lit tile sides. Where bolts overlap the brighter one wins.
        let bolt_reach = (BOLT_CORE_MM * 3.0 / CELL_MM).ceil() as usize;
        for s in segs {
            grid::for_each_near_seg(s.a, s.b, bolt_reach, |k| {
                let led = &LED_MAP[k];
                let d2 = seg_d2(s, led.wx, led.wy);
                if d2 < BOLT_CORE_MM * BOLT_CORE_MM * 9.0 {
                    let core = (BOLT_GAIN * (-d2 / (BOLT_CORE_MM * BOLT_CORE_MM)).exp()).min(1.0);
                    bolt_buf[k] = bolt_buf[k].max(s.env * core);
                }
            });
        }

        for (i, led) in LED_MAP.iter().enumerate() {
            let energy = energy_buf[i];

            // Water color, then twinkling froth above the threshold.
            let mut c = ramp(&WATER_RAMP, energy);
            let excess = ((energy - FOAM_THRESH) * FOAM_GAIN).clamp(0.0, 1.0);
            if excess > 0.0 {
                let k = (knuth_hash(led.chain_idx as u32) % PHASE_STEPS as u32) as usize;
                let tw = (ph_sin[k] * fp_cos + ph_cos[k] * fp_sin) * 0.5 + 0.5;
                c = mix_rgb(c, FOAM_COLOR, excess * tw * tw);
            }

            let density = density_buf[i].min(1.0);
            c = mix_rgb(c, CLOUD_COLOR, density * CLOUD_BRIGHT);

            let bolt = bolt_buf[i];
            if bolt > 0.0 {
                c = mix_rgb(c, FLASH_COLOR, bolt);
            }

            out[i] = [c[0] as u8, c[1] as u8, c[2] as u8];
        }
    }
}

/// Puts the cloud just off a random edge, heading for a random point near the middle.
fn respawn(c: &mut Cloud, rng: &mut Rng, t_ms: u32) {
    let along = rng.f32();
    let (x, y) = match rng.next_u32() % 4 {
        0 => (lerp(WORLD_LEFT, WORLD_RIGHT, along), WORLD_TOP - CLOUD_MARGIN_MM + 1.0),
        1 => (lerp(WORLD_LEFT, WORLD_RIGHT, along), WORLD_BOT + CLOUD_MARGIN_MM - 1.0),
        2 => (WORLD_LEFT - CLOUD_MARGIN_MM + 1.0, lerp(WORLD_TOP, WORLD_BOT, along)),
        _ => (WORLD_RIGHT + CLOUD_MARGIN_MM - 1.0, lerp(WORLD_TOP, WORLD_BOT, along)),
    };
    let tx = lerp(WORLD_LEFT, WORLD_RIGHT, 0.25 + 0.5 * rng.f32());
    let ty = lerp(WORLD_TOP, WORLD_BOT, 0.25 + 0.5 * rng.f32());
    let (dx, dy) = (tx - x, ty - y);
    let len = (dx * dx + dy * dy).sqrt().max(1.0);
    let speed = lerp(CLOUD_SPEED_MIN, CLOUD_SPEED_MAX, rng.f32());
    c.x = x;
    c.y = y;
    c.vx = dx / len * speed;
    c.vy = dy / len * speed;
    c.tvx = c.vx;
    c.tvy = c.vy;
    c.retarget_start_ms = t_ms;
    c.retarget_len_ms = rng.range_u32(CLOUD_RETARGET_MIN_MS, CLOUD_RETARGET_MAX_MS);
}

/// Water color by energy.
const WATER_RAMP: [(f32, [f32; 3]); 6] = [
    (0.00, [2.0, 8.0, 31.0]),      // deep blue-black
    (0.32, [9.0, 37.0, 97.0]),     // navy-blue
    (0.58, [17.0, 84.0, 149.0]),   // ocean blue
    (0.78, [33.0, 144.0, 169.0]),  // teal-turquoise
    (0.91, [150.0, 215.0, 222.0]), // pale aqua
    (1.00, [240.0, 249.0, 251.0]), // foam white
];

/// World position of lattice vertex (row, index).
fn lattice_pos(r: i32, i: i32) -> (f32, f32) {
    let x = WORLD_CX + (i as f32 - (5 - r) as f32 / 2.0) * LATTICE_SIDE;
    (x, LATTICE_TOP_Y + r as f32 * LATTICE_ROW_H)
}

/// One lit bolt side, with the per-segment terms of the distance test worked out once.
#[derive(Clone, Copy)]
struct Seg {
    a:    (f32, f32),
    b:    (f32, f32),
    ex:   f32,
    ey:   f32,
    len2: f32,
    env:  f32,
}

impl Seg {
    const ZERO: Seg =
        Seg { a: (0.0, 0.0), b: (0.0, 0.0), ex: 0.0, ey: 0.0, len2: 1.0, env: 0.0 };

    fn new(a: (f32, f32), b: (f32, f32), env: f32) -> Self {
        let (ex, ey) = (b.0 - a.0, b.1 - a.1);
        Seg { a, b, ex, ey, len2: (ex * ex + ey * ey).max(1.0), env }
    }
}

/// Squared distance from (x, y) to the segment.
fn seg_d2(s: &Seg, x: f32, y: f32) -> f32 {
    let t = (((x - s.a.0) * s.ex + (y - s.a.1) * s.ey) / s.len2).clamp(0.0, 1.0);
    let (dx, dy) = (x - s.a.0 - s.ex * t, y - s.a.1 - s.ey * t);
    dx * dx + dy * dy
}
