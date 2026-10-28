use crate::audio::Audio;
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{LED_COUNT, LED_MAP};
use crate::patterns::glints::{GlintStyle, Glints};
use crate::patterns::{
    cycle, cycle_q16, hsv_int, tile_hash, time_step_ms, Frame, Pattern, Rng, TILE_HASH_STEPS,
};
use core::f32::consts::TAU;

// The wave of brightness moving out from the center.
const WAVE_SPEED: f32 = 60.0; // base wave speed, mm/s outward
const WAVELENGTH: f32 = 120.0; // mm per cycle
const WAVE_FALLOFF: f32 = 0.7; // below 1 widens the bright crests, above 1 narrows them

// Each dark ring gets its own brightness at its darkest, picked when it starts at the center.
const DARK_MIN: f32 = 0.0;
const DARK_MAX: f32 = 0.5;

// Wave speed: now and then a new target is picked and the wave eases to it.
const SPEED_SCALE_MIN: f32 = 0.5;
const SPEED_SCALE_MAX: f32 = 1.6;
const SPEED_CHANGE_MIN_MS: u32 = 6_000; // wait before the next target
const SPEED_CHANGE_MAX_MS: u32 = 14_000;
const SPEED_EASE_MS: i32 = 2_000; // roughly how long the change takes

const GLINTS: GlintStyle = GlintStyle {
    tries_per_sec: 14.0, // about half succeed, since brighter LEDs are likelier to take one
    min_life_ms: 250,
    max_life_ms: 450,
    white: 0.75,
};

// Hue by distance from the center, drifting over time.
const HUE_PERIOD_MS: u32 = 24_000; // time for the colors to move through one full cycle
const RADIAL_SPAN_MM: f32 = 350.0; // distance over which the hue covers the arc once
const HUE_SCATTER: f32 = 0.06; // per-LED hue scatter

// Spiral: sectors of hue twisted by radius and rotating. The twist swings one way and back.
const SPIRAL_ARMS: f32 = 3.0;
const SPIRAL_TWIST: f32 = 0.004; // most hue cycles added per mm of radius
const TWIST_PERIOD_MS: u32 = 45_000; // one full swing of the twist

// Hue arc in degrees. The hue goes up the arc and back down, so there is no seam.
const HUE_START: f32 = 180.0; // cyan
const HUE_SPAN: f32 = 140.0; // through blue and violet toward magenta
const SATURATION: f32 = 1.0;

// Hue in sixths of a turn and saturation, both x4096.
const HUE_START_Q: u32 = (HUE_START / 60.0 * 4096.0) as u32;
const HUE_SPAN_Q: u32 = (HUE_SPAN / 60.0 * 4096.0) as u32;
const SATURATION_Q: i32 = (SATURATION * 4096.0) as i32;

const WAVE_BITS: u32 = 8; // the wave table has 2^WAVE_BITS entries per cycle
const WAVE_STEPS: usize = 1 << WAVE_BITS;
const RING_BITS: u32 = 6; // 2^RING_BITS rings remembered, far more than fit on the fixture
const RING_SLOTS: usize = 1 << RING_BITS;

/// Radial shimmer: a wave of brightness moves out from the center, with glints on its
/// crests. The hue also goes by distance from the center, across a cool arc, with a slow
/// three-armed spiral on top.
pub struct Shimmer {
    // Positions are in wave or hue cycles x65536, so the low 16 bits are the fraction of a cycle.
    ring_pos: [u32; LED_COUNT], // each LED's distance from the center
    hue_base: [u32; LED_COUNT], // the fixed part of each LED's hue position
    dist_q4: [i32; LED_COUNT],  // distance in mm x16
    // One wave cycle, crest to trough and back, x4096.
    wave_table: [u16; WAVE_STEPS],
    // How far the wave has traveled. Its whole part numbers the rings.
    wave_phase: u32,
    phase_rem: u32, // remainder carried to the next advance
    ring_dark: [u8; RING_SLOTS],
    speed_q8: i32, // current speed in mm/s x256
    speed_target_q8: i32,
    speed_pick_ms: u32, // when the target was picked
    speed_gap_ms: u32,  // wait until the next pick
    rng: Rng,
    glints: Glints,
    last_ms: u32,
}

impl Shimmer {
    pub fn new() -> Self {
        let mut rng = Rng::new(0x6A09_E667);
        let ring_dark = core::array::from_fn(|_| random_dark(&mut rng));
        Shimmer {
            ring_pos: core::array::from_fn(|i| (DIST_C[i] / WAVELENGTH * 65536.0) as u32),
            hue_base: core::array::from_fn(|i| {
                let scatter = tile_hash(&LED_MAP[i]) as f32 / TILE_HASH_STEPS as f32;
                // Radial gradient, per-LED scatter, and the spiral's sectors.
                let position = DIST_C[i] / RADIAL_SPAN_MM
                    + (scatter - 0.5) * HUE_SCATTER
                    + SPIRAL_ARMS * THETA_C[i] / TAU;
                (position * 65536.0) as i32 as u32
            }),
            dist_q4: core::array::from_fn(|i| (DIST_C[i] * 16.0) as i32),
            wave_table: core::array::from_fn(|i| {
                let cos = (i as f32 / WAVE_STEPS as f32 * TAU).cos();
                (((cos + 1.0) * 0.5).max(0.0).powf(WAVE_FALLOFF) * 4096.0 + 0.5) as u16
            }),
            wave_phase: 0,
            phase_rem: 0,
            ring_dark,
            speed_q8: (WAVE_SPEED * 256.0) as i32,
            speed_target_q8: (WAVE_SPEED * 256.0) as i32,
            speed_pick_ms: 0,
            speed_gap_ms: 0,
            rng,
            glints: Glints::new(0xBB67_AE85, GLINTS),
            last_ms: 0,
        }
    }
}

impl Pattern for Shimmer {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_ms = time_step_ms(&mut self.last_ms, t_ms, 100);
        self.advance_wave(t_ms, dt_ms);

        // Moves every LED's hue position back, which drifts the gradient and turns the spiral.
        let hue_shift = cycle_q16(t_ms, HUE_PERIOD_MS).wrapping_neg();
        // The spiral's current twist, in cycles per mm x2^20.
        let twist =
            (SPIRAL_TWIST * (cycle(t_ms, TWIST_PERIOD_MS) * TAU).sin() * 1_048_576.0) as i32;

        for (i, px) in out.iter_mut().enumerate() {
            let brightness = self.brightness(i);
            let hue = self.hue(i, hue_shift, twist);
            *px = hsv_int(hue, SATURATION_Q, brightness);
        }

        self.glints.update(t_ms, out);
    }
}

impl Shimmer {
    /// Eases the speed toward its target, moves the wave, and picks a darkness for each new ring.
    fn advance_wave(&mut self, t_ms: u32, dt_ms: u32) {
        if t_ms.wrapping_sub(self.speed_pick_ms) >= self.speed_gap_ms {
            let scale = self.rng.range_f32(SPEED_SCALE_MIN, SPEED_SCALE_MAX);
            self.speed_target_q8 = (WAVE_SPEED * scale * 256.0) as i32;
            self.speed_pick_ms = t_ms;
            self.speed_gap_ms = self.rng.range_u32(SPEED_CHANGE_MIN_MS, SPEED_CHANGE_MAX_MS);
        }
        self.speed_q8 += (self.speed_target_q8 - self.speed_q8) * dt_ms as i32 / SPEED_EASE_MS;

        // Cycles x65536 = mm/s x256 * ms * 256 / (1000 * mm per cycle).
        let divisor = (WAVELENGTH * 1000.0) as u32;
        let total = self.speed_q8.max(0) as u32 * dt_ms * 256 + self.phase_rem;
        let old = self.wave_phase;
        self.wave_phase = old.wrapping_add(total / divisor);
        self.phase_rem = total % divisor;

        let mut ring = old >> 16;
        while ring != self.wave_phase >> 16 {
            ring = ring.wrapping_add(1) & 0xFFFF;
            self.ring_dark[ring as usize & (RING_SLOTS - 1)] = random_dark(&mut self.rng);
        }
    }

    /// The wave's brightness at LED `i`, 0-255.
    fn brightness(&self, i: usize) -> i32 {
        // The ring number changes at the crest, where every ring is fully lit, so it never shows.
        let phase = self.wave_phase.wrapping_sub(self.ring_pos[i]);
        let wave =
            self.wave_table[((phase >> (16 - WAVE_BITS)) as usize) & (WAVE_STEPS - 1)] as i32;
        let dark = self.ring_dark[((phase >> 16) as usize) & (RING_SLOTS - 1)] as i32;
        dark + (((255 - dark) * wave) >> 12)
    }

    /// LED `i`'s hue, in sixths of a turn x4096.
    fn hue(&self, i: usize, hue_shift: u32, twist: i32) -> i32 {
        let wind = ((twist * self.dist_q4[i]) >> 8) as u32; // hue cycles x65536
        let hue_pos = self.hue_base[i].wrapping_add(hue_shift).wrapping_add(wind);

        // Fold hue_pos into a triangle wave, so the hue goes up the arc and back down.
        let frac = hue_pos & 0xFFFF;
        let folded = if frac < 0x8000 {
            frac << 1
        } else {
            (0xFFFF - frac) << 1
        };
        (HUE_START_Q + ((HUE_SPAN_Q * folded) >> 16)) as i32
    }
}

/// A new ring's brightness at its darkest, 0-255.
fn random_dark(rng: &mut Rng) -> u8 {
    (rng.range_f32(DARK_MIN, DARK_MAX) * 255.0) as u8
}
