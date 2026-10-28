use crate::audio::Audio;
use crate::led::geom::{BOARD_COUNT, BOARD_Y_RANGE, DIST_APEX};
use crate::led::map::{LED_COUNT, LED_MAP, WORLD_BOT, WORLD_CX, WORLD_H};
use crate::led::world::half_width_at;
use crate::patterns::{
    cycle, cycle_q16, hash01, hash2, knuth_hash, phase_phasors, ramp, random_u32, Frame, Pattern,
    Phasor, PHASE_STEPS,
};
use core::f32::consts::{PI, TAU};

// Heat waves moving out from the bottom point, and the base heat under them.
const WAVE_SPEED: f32 = 100.0; // mm/s outward, first wave
const WAVELENGTH: f32 = 80.0; // mm per cycle, first wave
const SECOND_WAVE_STRENGTH: f32 = 0.5; // second wave's amplitude, relative to the first
const SECOND_WAVELENGTH_RATIO: f32 = 1.7; // second wave's wavelength, relative to the first
const SECOND_SPEED_RATIO: f32 = 0.63; // second wave's speed, relative to the first
const WAVE_FLOOR: f32 = 0.35; // brightness left in the wave troughs
const HEAT_BIAS: f32 = 0.45; // heat added everywhere
const COOL_TILT: f32 = 0.35; // heat taken off at the top of the fixture
const TILE_TILT: f32 = 0.25; // heat taken off at the top of each tile

// Flicker, and a slow swell of the whole flame.
const FLICKER_DEPTH: f32 = 0.35; // flicker swing around 1.0
const BREATHE_DEPTH: f32 = 0.15; // slow whole-flame swell
const BREATHE_PERIOD_MS: u32 = 3_700;
// Two unrelated periods, so the flicker looks like noise.
const FLICKER_PERIOD_MS: u32 = 628;
const FLICKER2_PERIOD_MS: u32 = 401;

// Smoke: dark wisps that rise through the flame and wander sideways.
const SMOKE_DARKEN: f32 = 0.7; // how dark a wisp's core gets
const SMOKE_BAND_MM: f32 = 250.0; // vertical spacing between wisps
const SMOKE_WIGGLE: f32 = 2.0; // sideways wiggle depth, in radians
const SMOKE_WIGGLE_MM: f32 = 300.0; // horizontal wavelength of the wiggle
const SMOKE_START: f32 = 0.72; // field value where a wisp starts
const SMOKE_FULL: f32 = 0.88; // field value where it is fully dark
const SMOKE_RISE_PERIOD_MS: u32 = 5_100; // time to climb one band spacing
const SMOKE_WIGGLE_PERIOD_MS: u32 = 8_900;

// Embers: sparks that start low in the triangle, rise and fade.
const EMBER_COUNT: usize = 5; // embers in flight at once
const EMBER_PERIOD_MS: u32 = 2_600; // base flight time
const EMBER_STAGGER_MS: u32 = 977; // added per slot, so they respawn out of step
const EMBER_START_MM: f32 = 6.0; // lowest start height, above the bottom point
const EMBER_START_RANGE_MM: f32 = 130.0; // start heights are spread over this much
const EMBER_RISE_MM: f32 = 260.0; // how far an ember climbs
const EMBER_WOBBLE_MM: f32 = 25.0; // sideways drift while rising
const EMBER_WOBBLES: f32 = 1.5; // side-to-side swings per flight
const EMBER_RADIUS_MM: f32 = 28.0;
const EMBER_HEAT: f32 = 0.55; // heat added at the ember's center

// Tile flares: now and then one triangle tile flares up.
const FLARE_LEN_MS: u32 = 2_000; // how long one tile's flare lasts
const FLARE_RISE: f32 = 6.0; // full heat after the first sixth of the flare
const FLARE_HEAT: f32 = 0.6; // heat added across the flaring tile

// Time from one flare's start to the next, at least FLARE_LEN_MS so flares do not overlap.
const FLARE_GAP_MIN_MS: u32 = 2_000;
const FLARE_GAP_MAX_MS: u32 = 10_000;
const _: () = assert!(FLARE_GAP_MIN_MS >= FLARE_LEN_MS);

const SECOND_WAVELENGTH: f32 = WAVELENGTH * SECOND_WAVELENGTH_RATIO;
const SECOND_SPEED: f32 = WAVE_SPEED * SECOND_SPEED_RATIO;
const WAVE_ANGLE_PER_MM: f32 = TAU / WAVELENGTH;
const SECOND_ANGLE_PER_MM: f32 = TAU / SECOND_WAVELENGTH;
// How long each wave takes to move one wavelength.
const WAVE_PERIOD_MS: u32 = (WAVELENGTH / WAVE_SPEED * 1000.0) as u32;
const SECOND_PERIOD_MS: u32 = (SECOND_WAVELENGTH / SECOND_SPEED * 1000.0) as u32;

// Q12 values are x4096, so 4096 is 1.0.
const ONE: i32 = 4096;
/// The two waves' weights in their mix, Q12.
const MIX1_Q: i32 = (4096.0 / (1.0 + SECOND_WAVE_STRENGTH)) as i32;
const MIX2_Q: i32 = (4096.0 * SECOND_WAVE_STRENGTH / (1.0 + SECOND_WAVE_STRENGTH)) as i32;
const WAVE_FLOOR_Q: i32 = (WAVE_FLOOR * 4096.0) as i32;
const HEAT_BIAS_Q: i32 = (HEAT_BIAS * 4096.0) as i32;
const FLICKER_HALF_Q: i32 = (FLICKER_DEPTH * 0.5 * 4096.0) as i32;
const SMOKE_DARKEN_Q: i32 = (SMOKE_DARKEN * 4096.0) as i32;
const SMOKE_START_Q: i32 = (SMOKE_START * 4096.0) as i32;
/// Maps SMOKE_START..SMOKE_FULL onto 0..1, Q12.
const SMOKE_SCALE_Q: i32 = (4096.0 / (SMOKE_FULL - SMOKE_START)) as i32;
/// SMOKE_WIGGLE in turns x65536.
const SMOKE_WIGGLE_TURNS: i32 = (SMOKE_WIGGLE / TAU * 65536.0) as i32;
const FLARE_HEAT_Q: f32 = FLARE_HEAT * 4096.0;
const EMBER_HEAT_Q: f32 = EMBER_HEAT * 4096.0;
/// Ember radius in 1/16 mm, its square, and 1/radius^2 scaled to give Q12.
const EMBER_R_Q4: i32 = (EMBER_RADIUS_MM * 16.0) as i32;
const EMBER_R2_Q8: i32 = EMBER_R_Q4 * EMBER_R_Q4;
const EMBER_INV_R2: i32 = (4096 << 16) / EMBER_R2_Q8;

/// Sine table entries per turn; a phase in turns x65536 indexes it by its top bits.
const SIN_BITS: u32 = 10;
const SIN_STEPS: usize = 1 << SIN_BITS;
/// Fire ramp table entries across heat 0..1, plus one for exactly 1.0.
const RAMP_BITS: u32 = 10;
const RAMP_STEPS: usize = (1 << RAMP_BITS) + 1;

/// Fire color by heat. The coolest stop is a dim ember, not black.
const FIRE_RAMP: [(f32, [f32; 3]); 6] = [
    (0.00, [25.0, 3.0, 0.0]),      // dim ember
    (0.30, [180.0, 10.0, 0.0]),    // deep red
    (0.55, [255.0, 110.0, 0.0]),   // orange
    (0.75, [255.0, 230.0, 40.0]),  // bright yellow
    (0.90, [255.0, 255.0, 200.0]), // white
    (1.00, [170.0, 210.0, 255.0]), // blue-white
];

/// One ember this frame: its center in 1/16 mm and its heat in Q12.
struct Ember {
    x: i32,
    y: i32,
    heat: i32,
}

/// The sin and cos of one angle as whole numbers x4096, so the per-LED math needs no floats.
#[derive(Clone, Copy)]
struct PhasorQ12 {
    sin: i16,
    cos: i16,
}

impl PhasorQ12 {
    fn new(angle: f32) -> Self {
        Phasor::new(angle).into()
    }

    /// sin(self + other), Q12.
    fn sin_plus(self, other: PhasorQ12) -> i32 {
        (self.sin as i32 * other.cos as i32 + self.cos as i32 * other.sin as i32) >> 12
    }
}

impl From<Phasor> for PhasorQ12 {
    fn from(p: Phasor) -> Self {
        PhasorQ12 {
            sin: q12(p.sin) as i16,
            cos: q12(p.cos) as i16,
        }
    }
}

/// Fire rising from the bottom point of the Triangel. Heat waves move out from that point,
/// with flicker, dark smoke wisps, rising embers and now and then a flaring triangle tile.
pub struct Flame {
    // sin and cos of each LED's phase in the two waves, Q12.
    wave1: [PhasorQ12; LED_COUNT],
    wave2: [PhasorQ12; LED_COUNT],
    // The same for its two flicker phases, Q12.
    flicker1: [PhasorQ12; LED_COUNT],
    flicker2: [PhasorQ12; LED_COUNT],
    // The smoke wiggle's phase by x, Q12, and the smoke field's phase by y, in turns x65536.
    wiggle: [PhasorQ12; LED_COUNT],
    smoke_y: [u16; LED_COUNT],
    // Heat taken off for the LED's height in the fixture and in its tile, Q12.
    cooling: [i16; LED_COUNT],
    // Position in 1/16 mm.
    x_q4: [i16; LED_COUNT],
    y_q4: [i16; LED_COUNT],
    sin_table: [i16; SIN_STEPS],
    ramp_table: [[u8; 3]; RAMP_STEPS],
    // The tile that flares, when its flare started, and how long until the next starts.
    flare_board: u32,
    flare_start_ms: u32,
    flare_gap_ms: u32,
    /// New each time the pattern comes on screen, so its embers differ.
    seed: u32,
}

impl Flame {
    pub fn new() -> Self {
        let flicker = phase_phasors();
        // Two hashed flicker phases per LED, so neighboring LEDs flicker out of step.
        let hash = |i: usize| knuth_hash(i as u32);
        let flicker1_step = |i: usize| (hash(i) % PHASE_STEPS as u32) as usize;
        let flicker2_step = |i: usize| ((hash(i) >> 16) % PHASE_STEPS as u32) as usize;
        Flame {
            wave1: core::array::from_fn(|i| PhasorQ12::new(DIST_APEX[i] * WAVE_ANGLE_PER_MM)),
            wave2: core::array::from_fn(|i| PhasorQ12::new(DIST_APEX[i] * SECOND_ANGLE_PER_MM)),
            flicker1: core::array::from_fn(|i| flicker[flicker1_step(i)].into()),
            flicker2: core::array::from_fn(|i| flicker[flicker2_step(i)].into()),
            wiggle: core::array::from_fn(|i| PhasorQ12::new(LED_MAP[i].wx / SMOKE_WIGGLE_MM * TAU)),
            smoke_y: core::array::from_fn(|i| {
                (LED_MAP[i].wy / SMOKE_BAND_MM * 65536.0) as u32 as u16
            }),
            cooling: core::array::from_fn(|i| {
                let led = &LED_MAP[i];
                let height = (WORLD_BOT - led.wy) / WORLD_H; // 0 at the apex, 1 at the top row
                let (y_min, y_max) = BOARD_Y_RANGE[led.board_id as usize];
                let local_height = (y_max - led.wy) / (y_max - y_min).max(1.0);
                q12(height * COOL_TILT + local_height * TILE_TILT) as i16
            }),
            x_q4: core::array::from_fn(|i| (LED_MAP[i].wx * 16.0).round() as i16),
            y_q4: core::array::from_fn(|i| (LED_MAP[i].wy * 16.0).round() as i16),
            sin_table: core::array::from_fn(|step| {
                q12((step as f32 / SIN_STEPS as f32 * TAU).sin()) as i16
            }),
            ramp_table: core::array::from_fn(|step| {
                fire_ramp(step as f32 / (1 << RAMP_BITS) as f32)
            }),
            flare_board: 1,
            flare_start_ms: 0,
            flare_gap_ms: 0,
            seed: 0,
        }
    }
}

impl Pattern for Flame {
    fn on_enter(&mut self, _t_ms: u32) {
        self.seed = random_u32();
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        // Each time term wraps at its own period first, so the f32 stays precise.
        let wave1_s = (t_ms % WAVE_PERIOD_MS) as f32 / 1000.0;
        let wave2_s = (t_ms % SECOND_PERIOD_MS) as f32 / 1000.0;
        let breathe_phase = cycle(t_ms, BREATHE_PERIOD_MS) * TAU;
        let breathe = q12(1.0 - BREATHE_DEPTH * (0.5 + 0.5 * breathe_phase.sin()));
        let smoke_rise = cycle_q16(t_ms, SMOKE_RISE_PERIOD_MS) as u16; // turns x65536

        // sin and cos of this frame's angle for the waves, the flicker and the smoke wiggle.
        let wave1_now = PhasorQ12::new(-wave1_s * WAVE_SPEED * WAVE_ANGLE_PER_MM);
        let wave2_now = PhasorQ12::new(-wave2_s * SECOND_SPEED * SECOND_ANGLE_PER_MM);
        let flicker1_now = PhasorQ12::new(cycle(t_ms, FLICKER_PERIOD_MS) * TAU);
        let flicker2_now = PhasorQ12::new(cycle(t_ms, FLICKER2_PERIOD_MS) * TAU);
        let wiggle_now = PhasorQ12::new(cycle(t_ms, SMOKE_WIGGLE_PERIOD_MS) * TAU);

        let embers = self.embers(t_ms);
        let flare_heat = self.update_flare(t_ms);

        for (i, led) in LED_MAP.iter().enumerate() {
            let wave1 = self.wave1[i].sin_plus(wave1_now);
            let wave2 = self.wave2[i].sin_plus(wave2_now);
            let wave = (((wave1 * MIX1_Q + wave2 * MIX2_Q) >> 12) + ONE) >> 1;
            let wave = WAVE_FLOOR_Q + (((ONE - WAVE_FLOOR_Q) * wave) >> 12);

            let smoke = self.smoke(i, smoke_rise, wiggle_now);

            let flicker1 = self.flicker1[i].sin_plus(flicker1_now);
            let flicker2 = self.flicker2[i].sin_plus(flicker2_now);
            let flicker = ONE + ((FLICKER_HALF_Q * (flicker1 + flicker2)) >> 12);

            // Nearby embers and the flaring tile.
            let mut event_heat = self.ember_heat(i, &embers);
            if led.board_id as u32 == self.flare_board {
                event_heat += flare_heat;
            }

            let lit = (((wave * flicker) >> 12) * breathe) >> 12;
            let heat = ((lit + HEAT_BIAS_Q - self.cooling[i] as i32 + event_heat) * smoke) >> 12;
            out[i] = self.ramp_table[(heat.clamp(0, ONE) >> (12 - RAMP_BITS)) as usize];
        }
    }
}

impl Flame {
    /// Embers start low inside the triangle, rise with a wobble and fade.
    fn embers(&self, t_ms: u32) -> [Ember; EMBER_COUNT] {
        core::array::from_fn(|slot| {
            let period = EMBER_PERIOD_MS + slot as u32 * EMBER_STAGGER_MS;
            let phase = cycle(t_ms, period);
            let hash = hash2(t_ms / period, self.seed ^ slot as u32);
            let y0 = WORLD_BOT - EMBER_START_MM - hash01(hash, 1) * EMBER_START_RANGE_MM;
            let half_width = half_width_at(y0);
            let x0 = WORLD_CX + (hash01(hash, 2) * 2.0 - 1.0) * half_width;
            let x = x0 + ((phase * EMBER_WOBBLES + hash01(hash, 3)) * TAU).sin() * EMBER_WOBBLE_MM;
            let y = y0 - phase * EMBER_RISE_MM;
            Ember {
                x: (x * 16.0).round() as i32,
                y: (y * 16.0).round() as i32,
                heat: (EMBER_HEAT_Q * (phase * PI).sin()) as i32,
            }
        })
    }

    /// Now and then a random tile flares up fast and settles slowly. Returns its heat, Q12.
    fn update_flare(&mut self, t_ms: u32) -> i32 {
        let mut flare_age = t_ms.wrapping_sub(self.flare_start_ms);
        if flare_age >= self.flare_gap_ms {
            self.flare_start_ms = t_ms;
            self.flare_gap_ms =
                FLARE_GAP_MIN_MS + random_u32() % (FLARE_GAP_MAX_MS - FLARE_GAP_MIN_MS + 1);
            self.flare_board = 1 + random_u32() % BOARD_COUNT as u32;
            flare_age = 0;
        }
        let flare_level = if flare_age < FLARE_LEN_MS {
            let progress = flare_age as f32 / FLARE_LEN_MS as f32;
            (progress * FLARE_RISE).min(1.0) * (1.0 - progress)
        } else {
            0.0
        };
        (FLARE_HEAT_Q * flare_level) as i32
    }

    /// Smoke: rising bands that wiggle sideways, dark where the field is near its peak.
    fn smoke(&self, i: usize, smoke_rise: u16, wiggle_now: PhasorQ12) -> i32 {
        let wiggle = self.wiggle[i].sin_plus(wiggle_now);
        let turn = self.smoke_y[i]
            .wrapping_add(smoke_rise)
            .wrapping_add(((wiggle * SMOKE_WIGGLE_TURNS) >> 12) as u16);
        let field = (self.sine(turn) + ONE) >> 1;
        let wisp = (((field - SMOKE_START_Q) * SMOKE_SCALE_Q) >> 12).clamp(0, ONE);
        let wisp = (((wisp * wisp) >> 12) * (3 * ONE - 2 * wisp)) >> 12; // smoothstep
        ONE - ((SMOKE_DARKEN_Q * wisp) >> 12)
    }

    /// Sine table lookup, interpolated between entries.
    fn sine(&self, turn: u16) -> i32 {
        let step = (turn >> (16 - SIN_BITS)) as usize;
        let below = self.sin_table[step] as i32;
        let above = self.sin_table[(step + 1) & (SIN_STEPS - 1)] as i32;
        let part = (turn & ((1 << (16 - SIN_BITS)) - 1)) as i32;
        below + (((above - below) * part) >> (16 - SIN_BITS))
    }

    /// Heat from the embers near LED `i`, Q12.
    fn ember_heat(&self, i: usize, embers: &[Ember]) -> i32 {
        let mut total = 0;
        for ember in embers {
            let dx = self.x_q4[i] as i32 - ember.x;
            let dy = self.y_q4[i] as i32 - ember.y;
            if dx.abs() >= EMBER_R_Q4 || dy.abs() >= EMBER_R_Q4 {
                continue;
            }
            let d2 = dx * dx + dy * dy;
            if d2 < EMBER_R2_Q8 {
                let fall = ((EMBER_R2_Q8 - d2) * EMBER_INV_R2) >> 16;
                total += (ember.heat * fall) >> 12;
            }
        }
        total
    }
}

/// A float in -1..1 as Q12.
fn q12(v: f32) -> i32 {
    (v * 4096.0).round() as i32
}

/// Color for a heat in 0..1.
fn fire_ramp(heat: f32) -> [u8; 3] {
    ramp(&FIRE_RAMP, heat).map(|v| v as u8)
}
