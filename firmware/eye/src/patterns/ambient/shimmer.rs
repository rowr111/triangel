use crate::patterns::glints::{GlintStyle, Glints};
use crate::patterns::{Frame, Pattern, Rng, cycle, tile_hash, TILE_HASH_STEPS};

use crate::led::geom::{DIST_C, THETA_C};
use crate::audio::Audio;
use crate::led::map::{LED_COUNT, LED_MAP};
use core::f32::consts::TAU;

// Radial shimmer: a wave of brightness moves out from the center, with glints on its
// crests. The hue also goes by distance from the center, across a cool arc, with a slow
// three-armed spiral on top.
const DRIFT_PERIOD_MS: u32 = 24_000; // one full hue drift
const RADIAL_SPAN_MM:  f32 = 350.0;  // distance over which the hue covers the arc once
const HASH_JITTER:     f32 = 0.06;   // per-LED hue scatter
const WAVE_FALLOFF:    f32 = 0.7;    // below 1 widens the bright crests, above 1 narrows them
const WAVE_BITS:       u32 = 8;      // the wave table has 2^WAVE_BITS entries per cycle
const WAVE_STEPS:      usize = 1 << WAVE_BITS;

// Each dark ring gets its own brightness at its darkest, picked when it starts at the center.
const DARK_MIN:   f32   = 0.0;
const DARK_MAX:   f32   = 0.5;
const RING_SLOTS: usize = 64;  // rings remembered, far more than fit on the fixture

// Wave speed: now and then a new target is picked and the wave eases to it.
const SPEED_MIN:           f32 = 0.5;   // as a multiple of the base speed
const SPEED_MAX:           f32 = 1.6;
const SPEED_CHANGE_MIN_MS: u32 = 6_000; // wait before the next target
const SPEED_CHANGE_MAX_MS: u32 = 14_000;
const SPEED_EASE_MS:       i32 = 2_000; // roughly how long the change takes

const GLINTS: GlintStyle = GlintStyle {
    tries_per_sec: 14.0, // about half land, since brighter LEDs are likelier to take one
    min_ms:        250,  // fade time
    max_ms:        450,
    white:         0.75, // how far toward white a glint starts
};

// Hue arc in degrees. The hue goes up the arc and back down, so there is no seam.
const HUE_START:  f32 = 180.0; // cyan
const HUE_SPAN:   f32 = 140.0; // through blue and violet toward magenta
const SATURATION: f32 = 1.0;

// Hue in sixths of a turn and saturation, both x4096.
const HUE_START_Q: u32 = (HUE_START / 60.0 * 4096.0) as u32;
const HUE_SPAN_Q:  u32 = (HUE_SPAN / 60.0 * 4096.0) as u32;
const SATURATION_Q: i32 = (SATURATION * 4096.0) as i32;

// Spiral: sectors of hue twisted by radius and rotating. The twist swings one way and back.
const SPIRAL_ARMS:      f32 = 3.0;
const SPIRAL_TWIST:     f32 = 0.004;  // most hue cycles added per mm of radius
const SPIRAL_PERIOD_MS: u32 = 12_000; // one full rotation
const TWIST_PERIOD_MS:  u32 = 45_000; // one full swing of the twist

pub struct CenterShimmer {
    pub speed:      f32, // base wave speed, mm/s outward
    pub wavelength: f32, // mm per cycle
    // Positions are in wave or hue cycles x65536, so the low 16 bits are the fraction of a cycle.
    ring_pos: [u32; LED_COUNT], // each LED's distance from the center
    hue_base: [u32; LED_COUNT], // the fixed part of each LED's hue position
    dist_q4:  [i32; LED_COUNT], // distance in mm x16
    // One wave cycle, crest to trough and back, x4096.
    wave_lut: [u16; WAVE_STEPS],
    // How far the wave has traveled. Its whole part numbers the rings.
    wave_phase: u32,
    phase_rem:  u32, // remainder carried to the next advance
    ring_dark:  [u8; RING_SLOTS],
    speed_q8:        i32, // current speed in mm/s x256
    speed_target_q8: i32,
    speed_pick_ms:   u32, // when the target was picked
    speed_gap_ms:    u32, // wait until the next pick
    rng: Rng,
    glints: Glints,
    last_ms: u32,
}

impl CenterShimmer {
    pub fn new(speed: f32, wavelength: f32) -> Self {
        let mut s = CenterShimmer {
            speed,
            wavelength,
            ring_pos: core::array::from_fn(|i| (DIST_C[i] / wavelength * 65536.0) as u32),
            hue_base: core::array::from_fn(|i| {
                let hn = tile_hash(&LED_MAP[i]) as f32 / TILE_HASH_STEPS as f32;
                // Radial gradient, per-LED scatter, and the spiral's sectors.
                let p = DIST_C[i] / RADIAL_SPAN_MM
                    + (hn - 0.5) * HASH_JITTER
                    + SPIRAL_ARMS * THETA_C[i] / TAU;
                (p * 65536.0) as i32 as u32
            }),
            dist_q4: core::array::from_fn(|i| (DIST_C[i] * 16.0) as i32),
            wave_lut: core::array::from_fn(|i| {
                let c = (i as f32 / WAVE_STEPS as f32 * TAU).cos();
                (((c + 1.0) * 0.5).max(0.0).powf(WAVE_FALLOFF) * 4096.0 + 0.5) as u16
            }),
            wave_phase: 0,
            phase_rem: 0,
            ring_dark: [0; RING_SLOTS],
            speed_q8: (speed * 256.0) as i32,
            speed_target_q8: (speed * 256.0) as i32,
            speed_pick_ms: 0,
            speed_gap_ms: 0,
            rng: Rng::new(0x6A09_E667),
            glints: Glints::new(0xBB67_AE85, GLINTS),
            last_ms: 0,
        };
        for n in 0..RING_SLOTS {
            s.ring_dark[n] = s.random_dark();
        }
        s
    }

    /// A new ring's brightness at its darkest, 0-255.
    fn random_dark(&mut self) -> u8 {
        ((DARK_MIN + self.rng.f32() * (DARK_MAX - DARK_MIN)) * 255.0) as u8
    }

    /// Eases the speed toward its target, moves the wave, and picks a darkness for each new ring.
    fn advance_wave(&mut self, t_ms: u32, dt_ms: u32) {
        if t_ms.wrapping_sub(self.speed_pick_ms) >= self.speed_gap_ms {
            let m = SPEED_MIN + self.rng.f32() * (SPEED_MAX - SPEED_MIN);
            self.speed_target_q8 = (self.speed * m * 256.0) as i32;
            self.speed_pick_ms = t_ms;
            self.speed_gap_ms = SPEED_CHANGE_MIN_MS
                + (self.rng.f32() * (SPEED_CHANGE_MAX_MS - SPEED_CHANGE_MIN_MS) as f32) as u32;
        }
        self.speed_q8 += (self.speed_target_q8 - self.speed_q8) * dt_ms as i32 / SPEED_EASE_MS;

        // Cycles x65536 = mm/s x256 * ms * 256 / (1000 * mm per cycle).
        let per = (self.wavelength * 1000.0) as u32;
        let num = self.speed_q8.max(0) as u32 * dt_ms * 256 + self.phase_rem;
        let old = self.wave_phase;
        self.wave_phase = old.wrapping_add(num / per);
        self.phase_rem = num % per;

        let mut ring = old >> 16;
        while ring != self.wave_phase >> 16 {
            ring = ring.wrapping_add(1) & 0xFFFF;
            self.ring_dark[ring as usize & (RING_SLOTS - 1)] = self.random_dark();
        }
    }

}

/// HSV to RGB in integer math. `h6` is the hue in sixths of a turn x4096, `v` is 0-255.
fn hsv_q(h6: i32, v: i32) -> [u8; 3] {
    let f = |n: i32| -> u8 {
        let mut k = n * 4096 + h6;
        if k >= 6 * 4096 {
            k -= 6 * 4096;
        }
        let c = k.min(4 * 4096 - k).clamp(0, 4096);
        ((v * (4096 - ((SATURATION_Q * c) >> 12))) >> 12) as u8
    };
    [f(5), f(3), f(1)]
}

impl Pattern for CenterShimmer {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let dt_ms = t_ms.wrapping_sub(self.last_ms).min(100);
        self.last_ms = t_ms;
        self.advance_wave(t_ms, dt_ms);

        // Hue drift and spiral rotation, both 0..1 x65536.
        let drift = (t_ms % DRIFT_PERIOD_MS) * 65536 / DRIFT_PERIOD_MS;
        let spin = (t_ms % SPIRAL_PERIOD_MS) * 65536 / SPIRAL_PERIOD_MS;
        let hue_shift = drift.wrapping_sub(spin);
        // The spiral's current twist, in cycles per mm x2^20.
        let twist = (SPIRAL_TWIST
            * (cycle(t_ms, TWIST_PERIOD_MS) * TAU).sin()
            * 1_048_576.0) as i32;

        for (i, o) in out.iter_mut().enumerate() {
            // The ring number changes at the crest, where every ring is at full brightness,
            // so the change never shows.
            let x = self.wave_phase.wrapping_sub(self.ring_pos[i]);
            let wave = self.wave_lut[((x >> (16 - WAVE_BITS)) as usize) & (WAVE_STEPS - 1)] as i32;
            let dark = self.ring_dark[((x >> 16) as usize) & (RING_SLOTS - 1)] as i32;

            let v = dark + (((255 - dark) * wave) >> 12);

            let wind = ((twist * self.dist_q4[i]) >> 8) as u32;
            let p = self.hue_base[i].wrapping_add(hue_shift).wrapping_add(wind);

            // Fold p into a triangle wave, so the hue goes up the arc and back down.
            let frac = p & 0xFFFF;
            let tri = if frac < 0x8000 { frac << 1 } else { (0xFFFF - frac) << 1 };
            let hue = HUE_START_Q + ((HUE_SPAN_Q * tri) >> 16);
            *o = hsv_q(hue as i32, v);
        }

        self.glints.update(t_ms, dt_ms, out);
    }
}
