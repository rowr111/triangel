use crate::patterns::{Frame, Pattern, lerp, phase_phasors, PHASE_STEPS};
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{Led, WORLD_BOT, WORLD_CX, WORLD_H, WORLD_TOP, LED_COUNT, LED_MAP};
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
const WATER_FLOOR: f32 = 0.40;
const BOIL_DEPTH:  f32 = 0.10;
const BOIL_CELL_MM: f32 = 140.0;    // spatial scale of the churn
const BOIL_PERIOD_MS:  u32 = 4_900; // two incommensurate phases so the churn
const BOIL2_PERIOD_MS: u32 = 3_100; // reads as boiling, not a sweep
const BOIL_CROSS_Y: f32 = 0.83; // frequency of each axis' modulating sine, relative to the
const BOIL_CROSS_X: f32 = 0.71; // cell scale - off-integer so the two never lock together

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

// Lightning: pure white bolts streaking along the tile lattice - the path builds one
// tile side at a time and stays fully lit; on reaching its endpoint it stutters
// through flickering return-stroke pulses, then dies to dark. A few independent
// slots roll their own irregular gaps, so bolts occasionally overlap.
const BOLT_SLOTS: usize = 2;
const STRIKE_GAP_MIN_MS: u32 = 4_000;  // per slot; overall cadence scales with BOLT_SLOTS
const STRIKE_GAP_MAX_MS: u32 = 14_000;
const BOLT_SEGS_MIN: usize = 3;      // bolt length range, in tile edges walked
const BOLT_SEGS_MAX: usize = 8;
const BOLT_STEP_MS: u32 = 150;       // leader advances one tile side per step
const FLICKER_FADE_MS: f32 = 180.0;  // decay of each full-path return-stroke pulse
const BOLT_PULSES_MIN: usize = 2;    // return strokes per strike...
const BOLT_PULSES_MAX: usize = 5;    // ...rolled fresh each time, irregularly spaced
const BOLT_TAIL_MS: u32 = 1_400;     // fade-out allowance after the final pulse
const BOLT_CORE_MM: f32 = 12.0;      // catches the two LED rows straddling a tile edge
const BOLT_GAIN: f32 = 1.6;          // spatial overdrive: near-line LEDs saturate to full white

// Tile-corner lattice the bolts walk: vertex row r (0 = wide top edge, 5 = bottom apex)
// holds 6-r vertices centered on WORLD_CX. Side and row height include the inter-tile
// gap, matching the previewer's board grid to within a few mm.
const LATTICE_TOP_Y: f32 = 1.0;
const LATTICE_ROW_H: f32 = 89.0;
const LATTICE_SIDE:  f32 = 103.5;
// Six lattice neighbors as (row, index) deltas: left, right, and the four diagonals.
const LATTICE_NEIGHBORS: [(i32, i32); 6] = [(0, -1), (0, 1), (-1, 0), (-1, 1), (1, -1), (1, 0)];

const FOAM_COLOR:  [f32; 3] = [235.0, 245.0, 250.0];
const CLOUD_COLOR: [f32; 3] = [168.0, 178.0, 196.0]; // pale blue-gray
const FLASH_COLOR: [f32; 3] = [255.0, 255.0, 255.0]; // lightning is pure white

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

/// A bolt's path: a walk along the tile lattice, at most BOLT_SEGS_MAX edges long.
struct Bolt {
    pts: [(f32, f32); BOLT_SEGS_MAX + 1],
    len: usize, // points in use (edges walked + 1)
}

struct Strike {
    start_ms: u32,
    bolt:     Bolt,
    // Return-stroke onsets relative to start_ms; the first is the moment the leader
    // completes its walk. u32::MAX marks an unused slot.
    pulse_ms: [u32; BOLT_PULSES_MAX],
}

/// One independent lightning scheduler: its own irregular gap countdown and at most
/// one bolt in flight. Slots sometimes land close together - overlapping streaks.
struct StrikeSlot {
    strike:       Option<Strike>,
    gap_start_ms: u32,
    gap_len_ms:   u32,
}

/// Boiling sea under a storm: dark water bubbling with upwellings, pale clouds drifting
/// on wandering headings, and sparse lightning. Stateful (cloud simulation plus a seeded
/// PRNG) unlike the hash-scheduled ambients, so headings genuinely change over time.
pub struct Squall {
    clouds:  [Cloud; CLOUD_COUNT],
    slots:   [StrikeSlot; BOLT_SLOTS],
    rng:     u32,
    last_ms: Option<u32>, // previous frame time; None until the first render
    // Phasors for the boil's two inner sines. Their arguments are a fixed per-LED term plus a
    // per-frame one, so rotating these replaces two of the four sin calls per LED.
    by_sin: [f32; LED_COUNT],
    by_cos: [f32; LED_COUNT],
    bx_sin: [f32; LED_COUNT],
    bx_cos: [f32; LED_COUNT],
    // Per-LED totals for this frame, filled by the scatter passes.
    energy:  [f32; LED_COUNT],
    density: [f32; LED_COUNT],
    bolt:    [f32; LED_COUNT],
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

    /// Per slot: retire a bolt whose last side has fully faded, then ignite the next
    /// once that slot's irregular gap elapses.
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
                    roll_range(&mut self.rng, STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
            }
            if self.slots[k].strike.is_none()
                && t_ms.wrapping_sub(self.slots[k].gap_start_ms) >= self.slots[k].gap_len_ms
            {
                let strike = self.ignite(t_ms);
                self.slots[k].strike = Some(strike);
            }
        }
    }

    /// Anchor a new bolt to a random cloud, walk its lattice path, and roll its return
    /// strokes: the first lands the moment the leader completes, then the lit path
    /// stutters through up to four more irregularly spaced pulses.
    fn ignite(&mut self, t_ms: u32) -> Strike {
        let a = xorshift(&mut self.rng) as usize % CLOUD_COUNT;
        let (cx, cy) = (self.clouds[a].x, self.clouds[a].y);
        let bolt = self.walk_bolt(cx, cy);
        let complete = (bolt.len as u32 - 1) * BOLT_STEP_MS;
        let n = BOLT_PULSES_MIN
            + xorshift(&mut self.rng) as usize % (BOLT_PULSES_MAX - BOLT_PULSES_MIN + 1);
        let mut pulse_ms = [u32::MAX; BOLT_PULSES_MAX];
        let mut at = complete;
        for p in pulse_ms.iter_mut().take(n) {
            *p = at;
            at += 60 + xorshift(&mut self.rng) % 360; // uneven stutter, new every strike
        }
        Strike { start_ms: t_ms, bolt, pulse_ms }
    }

    /// Random-walk a bolt along the tile lattice, starting at the vertex nearest the
    /// flashing cloud. Each hop must keep moving forward (positive dot with the incoming
    /// heading): straight ahead or a gentle 60-degree turn, never the backtrack or the
    /// double-back alongside it. A cornered walk ends early - a streak out of sky.
    fn walk_bolt(&mut self, cx: f32, cy: f32) -> Bolt {
        let r0 = (((cy - LATTICE_TOP_Y) / LATTICE_ROW_H).round() as i32).clamp(0, 5);
        let i0 = (((cx - WORLD_CX) / LATTICE_SIDE + (5 - r0) as f32 / 2.0).round() as i32)
            .clamp(0, 5 - r0);
        let segs = BOLT_SEGS_MIN
            + xorshift(&mut self.rng) as usize % (BOLT_SEGS_MAX - BOLT_SEGS_MIN + 1);

        let (mut r, mut i) = (r0, i0);
        let mut pts = [(0.0f32, 0.0f32); BOLT_SEGS_MAX + 1];
        pts[0] = lattice_pos(r, i);
        let mut len = 1;
        let (mut hx, mut hy) = (0.0f32, 0.0f32); // incoming unit heading; unset on first hop
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
                break; // cornered at the canvas edge - the bolt ends here
            }
            let (nr, ni) = cand[xorshift(&mut self.rng) as usize % n];
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
                    for slot in &mut self.slots {
                        slot.gap_start_ms = t_ms;
                    }
                    for c in &mut self.clouds {
                        c.retarget_start_ms = t_ms;
                    }
                }
                (dt_ms as f32 / 1000.0).min(0.1)
            }
            None => {
                // First frame: anchor the strike countdowns so the storm opens quietly.
                let Squall { slots, rng, .. } = self;
                for slot in slots.iter_mut() {
                    slot.gap_start_ms = t_ms;
                    slot.gap_len_ms = roll_range(rng, STRIKE_GAP_MIN_MS, STRIKE_GAP_MAX_MS);
                }
                0.0
            }
        };
        self.last_ms = Some(t_ms);
        self.update_clouds(t_ms, dt_s);
        self.update_strikes(t_ms);

        // Bloom slots: each period, a slot hash-picks a spot near the triangle and lives
        // one bloom there - radius grows birth-to-death while a sin envelope swells and
        // fades the energy. (x, y, 1/r^2, amplitude, ring blend, grid reach) per slot.
        let mut blooms = [(0.0f32, 0.0f32, 0.0f32, 0.0f32, 0.0f32, 0usize); BLOOM_SLOTS];
        for (k, b) in blooms.iter_mut().enumerate() {
            let period = BLOOM_PERIOD_MS + k as u32 * BLOOM_STAGGER_MS;
            let cycle = t_ms / period;
            let p = (t_ms % period) as f32 / period as f32;
            let h = cycle.wrapping_mul(2654435761) ^ (k as u32).wrapping_mul(0x9E37_79B9);
            let y = WORLD_TOP + ((h >> 10) % 1000) as f32 / 1000.0 * WORLD_H;
            let hw = (WORLD_BOT - y) / WORLD_H * WORLD_HALF_W + 40.0; // triangle width at y
            let x = WORLD_CX + ((h % 1000) as f32 / 1000.0 * 2.0 - 1.0) * hw;
            let r = lerp(BLOOM_R0_MM, BLOOM_R1_MM, p);
            let reach = (r / CELL_MM).ceil() as usize;
            *b = (x, y, 1.0 / (r * r), BLOOM_PEAK * (p * PI).sin(), BLOOM_RIM * p, reach);
        }

        // Cloud lobes, flattened across clouds: each orbits its cloud center and breathes
        // its radius on seed-derived periods, so the silhouette slowly morphs.
        // (x, y, 1/r^2, grid reach).
        let mut lobes = [(0.0f32, 0.0f32, 0.0f32, 0usize); CLOUD_COUNT * CLOUD_LOBES];
        for (ci, c) in self.clouds.iter().enumerate() {
            for j in 0..CLOUD_LOBES {
                let h = c.seed ^ (j as u32).wrapping_mul(0x9E37_79B9);
                let orbit_p = 9_000 + h % 7_000;
                let breathe_p = 3_500 + (h >> 8) % 3_000;
                let orbit = (t_ms % orbit_p) as f32 / orbit_p as f32 * TAU + (h >> 16) as f32;
                let breathe = (t_ms % breathe_p) as f32 / breathe_p as f32 * TAU + (h >> 12) as f32;
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

        // Active bolt segments with envelopes (a, b, envelope). While the leader crawls,
        // every side it has reached stays fully lit, so the bolt visibly builds. Once
        // the walk completes, the whole path shares the return-stroke envelope:
        // full-white pops dipping between pulses, dying out after the last one.
        let mut segs = [Seg::ZERO; BOLT_SLOTS * BOLT_SEGS_MAX];
        let mut n_segs = 0;
        for slot in &self.slots {
            let Some(s) = &slot.strike else { continue };
            let el = t_ms.wrapping_sub(s.start_ms);
            if el < s.pulse_ms[0] {
                // Leader phase: every side reached so far holds at full strength.
                let reached = (el / BOLT_STEP_MS) as usize + 1;
                for w in s.bolt.pts[..(reached + 1).min(s.bolt.len)].windows(2) {
                    segs[n_segs] = Seg::new(w[0], w[1], 1.0);
                    n_segs += 1;
                }
            } else {
                // Flicker phase: every fired pulse decays; the loudest one wins.
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

        // Per-frame rotation angles for the boil's inner sines, and the cell scale as one
        // multiply rather than a divide plus a multiply per LED.
        let boil_k = TAU / BOIL_CELL_MM;
        let (b1_sin, b1_cos) = boil1.sin_cos();
        let (b2_sin, b2_cos) = boil2.sin_cos();
        let (fp_sin, fp_cos) = foam_phase.sin_cos();
        let (ph_sin, ph_cos) = phase_phasors();

        // Boiling floor, seeded into the energy buffer. The inner sines come from the phasor
        // tables; the outer two can't factor, since their arguments contain the inner results.
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
        for (i, led) in leds.iter().enumerate() {
            let bx = led.wx * boil_k;
            let by = led.wy * boil_k;
            let mod_y = by_sin[i] * b2_cos + by_cos[i] * b2_sin;
            let mod_x = bx_sin[i] * b1_cos + bx_cos[i] * b1_sin;
            let boil = ((bx + mod_y + boil1).sin() + (by + mod_x + boil2).sin()) * 0.5;
            energy_buf[i] = WATER_FLOOR + BOIL_DEPTH * boil;
            density_buf[i] = 0.0;
            bolt_buf[i] = 0.0;
        }

        // Upwellings: a young bloom is a filled swell; the rim blend morphs it toward a ring as
        // it ages, so it dies ghosting outward instead of switching off. Scattered onto the LEDs
        // each bloom reaches, in slot order, so every LED sums them in the order it used to.
        for &(x, y, inv_r2, amp, ring, reach) in &blooms {
            grid::for_each_near(x, y, reach, |k| {
                let led = &leds[k];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    let filled = (1.0 - q) * (1.0 - q);
                    let ringed = 4.0 * q * (1.0 - q);
                    energy_buf[k] += amp * lerp(filled, ringed, ring);
                }
            });
        }

        // Cloud cover, scattered the same way: pale mass over the water, saturating where
        // lobes overlap.
        for &(x, y, inv_r2, reach) in &lobes {
            grid::for_each_near(x, y, reach, |k| {
                let led = &leds[k];
                let q = ((led.wx - x).powi(2) + (led.wy - y).powi(2)) * inv_r2;
                if q < 1.0 {
                    density_buf[k] += (1.0 - q) * (1.0 - q);
                }
            });
        }

        // Lightning: pure white along the lit tile sides, scattered onto the narrow ribbon of
        // cells each side passes through. Taking the max means visit order does not matter.
        let bolt_reach = (BOLT_CORE_MM * 3.0 / CELL_MM).ceil() as usize;
        for s in segs {
            grid::for_each_near_seg(s.a, s.b, bolt_reach, |k| {
                let led = &leds[k];
                let d2 = seg_d2(s, led.wx, led.wy);
                if d2 < BOLT_CORE_MM * BOLT_CORE_MM * 9.0 {
                    let core = (BOLT_GAIN * (-d2 / (BOLT_CORE_MM * BOLT_CORE_MM)).exp()).min(1.0);
                    bolt_buf[k] = bolt_buf[k].max(s.env * core);
                }
            });
        }

        for (i, led) in leds.iter().enumerate() {
            let energy = energy_buf[i];

            // Water color, then froth where the crest breaks: hash-twinkled speckle.
            let mut c = water_ramp(energy);
            let excess = ((energy - FOAM_THRESH) * FOAM_GAIN).clamp(0.0, 1.0);
            if excess > 0.0 {
                let h = (led.chain_idx as u32).wrapping_mul(2654435761);
                let k = (h % PHASE_STEPS as u32) as usize;
                let tw = (ph_sin[k] * fp_cos + ph_cos[k] * fp_sin) * 0.5 + 0.5;
                c = mix(c, FOAM_COLOR, excess * tw * tw);
            }

            let density = density_buf[i].min(1.0);
            c = mix(c, CLOUD_COLOR, density * CLOUD_BRIGHT);

            // The overdrive saturates the spatial falloff, so the LED rows straddling the line
            // hit true full white while the pulse envelope keeps its whole flicker depth.
            let bolt = bolt_buf[i];
            if bolt > 0.0 {
                c = mix(c, FLASH_COLOR, bolt);
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

/// Storm-water ramp: deep blue-black up through turquoise to foam white. A swell
/// crest falls off white -> aqua -> turquoise -> blue over its upper half, while
/// the resting floor keeps a moody navy-blue depth.
fn water_ramp(e: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.00, [2.0, 8.0, 31.0]),      // deep blue-black
        (0.32, [9.0, 37.0, 97.0]),     // navy-blue
        (0.58, [17.0, 84.0, 149.0]),   // ocean blue
        (0.78, [33.0, 144.0, 169.0]),  // teal-turquoise
        (0.91, [150.0, 215.0, 222.0]), // pale aqua
        (1.00, [240.0, 249.0, 251.0]), // foam white
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

/// World position of lattice vertex (row, index): row 0 is the wide top edge, row 5
/// the bottom apex; row r holds 6-r vertices centered on WORLD_CX.
fn lattice_pos(r: i32, i: i32) -> (f32, f32) {
    let x = WORLD_CX + (i as f32 - (5 - r) as f32 / 2.0) * LATTICE_SIDE;
    (x, LATTICE_TOP_Y + r as f32 * LATTICE_ROW_H)
}

/// Squared distance from (x, y) to the segment a-b.
/// One lit bolt side, with the direction terms of the distance test hoisted off the per-LED
/// path - they depend only on the segment.
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

fn seg_d2(s: &Seg, x: f32, y: f32) -> f32 {
    let t = (((x - s.a.0) * s.ex + (y - s.a.1) * s.ey) / s.len2).clamp(0.0, 1.0);
    let (dx, dy) = (x - s.a.0 - s.ex * t, y - s.a.1 - s.ey * t);
    dx * dx + dy * dy
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
