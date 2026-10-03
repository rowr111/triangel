use crate::patterns::glints::{GlintStyle, Glints};
use crate::patterns::{Frame, Pattern};

use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, LED_COUNT, LED_MAP};
use core::f32::consts::TAU;

// Radial shimmer: a wave ripples out from the center with brief glints flashing on its
// crests. Color is a vibrant (fully-saturated) hue swept across a cool arc, also by distance
// from the center, so the motion and the color share one source.
//
// Everything worked out per LED is integer math; the chip has no floating-point unit.
const DRIFT_PERIOD_MS: u32 = 24_000; // one full hue drift - rotates the bands visibly
const RADIAL_SPAN_MM:  f32 = 350.0;  // distance over which the hue sweeps the arc once
                                     // (lower = more color bands across the fixture at once)
const HASH_JITTER:     f32 = 0.06;   // per-LED hue scatter, for a jeweled twinkle
const WAVE_FALLOFF:    f32 = 0.7;    // shapes the crest->trough falloff: <1 broadens the bright crests
                                     // (brighter overall, thin dark troughs); >1 sharpens to punchy peaks
const WAVE_BITS:       u32 = 8;      // one wave cycle is read from a table of 2^WAVE_BITS entries
const WAVE_STEPS:      usize = 1 << WAVE_BITS;

// Each dark ring gets its own brightness at its darkest, picked when it starts at the center.
const DARK_MIN:   f32   = 0.0; // 0 is fully black
const DARK_MAX:   f32   = 0.5;
const RING_SLOTS: usize = 64;  // rings remembered, far more than fit on the fixture at once

// Wave speed: every so often a new target is picked and the wave glides to it.
const SPEED_MIN:           f32 = 0.5;   // slowest, as a multiple of the base speed
const SPEED_MAX:           f32 = 1.6;   // fastest
const SPEED_CHANGE_MIN_MS: u32 = 6_000; // wait before the next target, picked each time
const SPEED_CHANGE_MAX_MS: u32 = 14_000;
const SPEED_EASE_MS:       i32 = 2_000; // roughly how long the glide takes

// Glints: brief near-white flashes on single LEDs, landing mostly on the bright crests.
const GLINT_TRIES_PER_SEC: f32   = 14.0; // each lands only as likely as its LED is bright, so about half do
const GLINT_MIN_MS:        u32   = 250;  // fade time, picked per glint
const GLINT_MAX_MS:        u32   = 450;
const GLINT_WHITE:         f32   = 0.75; // how far toward white a glint starts
const GLINTS: GlintStyle = GlintStyle {
    tries_per_sec: GLINT_TRIES_PER_SEC,
    min_ms:        GLINT_MIN_MS,
    max_ms:        GLINT_MAX_MS,
    white:         GLINT_WHITE,
};

// Vibrant cool hue arc, in degrees. Saturation is full (rainbow-level); the hue ping-pongs
// across [HUE_START, HUE_START + HUE_SPAN] so the bands stay in the cool range and never seam.
const HUE_START:  f32 = 180.0; // cyan
const HUE_SPAN:   f32 = 140.0; // up through blue/violet toward magenta, then back
const SATURATION: f32 = 1.0;   // 1.0 = max vibrance; lower for a softer, brighter look

// The same in fixed point for the per-LED color math: hue in sixths of a turn and
// saturation, both x4096.
const HUE_START_Q: u32 = (HUE_START / 60.0 * 4096.0) as u32;
const HUE_SPAN_Q:  u32 = (HUE_SPAN / 60.0 * 4096.0) as u32;
const SATURATION_Q: i32 = (SATURATION * 4096.0) as i32;

// Spiral: SPIRAL_ARMS sectors of hue, twisted by radius and spun over time, so they curl
// into arms that pinwheel outward - echoing the triangle's 3-fold shape. Layered on top of
// the radial field. The winding slowly swings from one direction to the other and back.
const SPIRAL_ARMS:      f32 = 3.0;    // arms around the wheel (3 matches the triangle)
const SPIRAL_TWIST:     f32 = 0.004;  // most hue cycles added per mm of radius - how tightly arms wind
const SPIRAL_PERIOD_MS: u32 = 12_000; // one full rotation of the pinwheel
const TWIST_PERIOD_MS:  u32 = 45_000; // one full swing of the winding, one way and back

// Distinct values the per-LED hash can take.
const HASH_STEPS: usize = 97;

pub struct CenterShimmer {
    pub speed:      f32, // mm/s outward wave propagation, the base the speed drifts around
    pub wavelength: f32, // mm per cycle
    // Positions below are in wave or hue cycles x65536, so the fraction of a cycle is the low
    // 16 bits and wrapping past a whole cycle is free.
    ring_pos: [u32; LED_COUNT], // each LED's distance from the center
    hue_base: [u32; LED_COUNT], // the part of each LED's hue position that never changes
    dist_q4:  [i32; LED_COUNT], // distance in mm x16, for the spiral winding
    // One wave cycle, crest to trough and back, shaped by WAVE_FALLOFF. x4096.
    wave_lut: [u16; WAVE_STEPS],
    // How far the wave has traveled. Its whole part numbers the rings.
    wave_phase: u32,
    phase_rem:  u32, // left over from the last advance, so rounding never loses any
    ring_dark:  [u8; RING_SLOTS],
    speed_q8:        i32, // current speed in mm/s x256
    speed_target_q8: i32,
    speed_pick_ms:   u32, // when the target was last picked, and the wait until the next
    speed_gap_ms:    u32,
    rng: u32,
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
                // Per-LED hue scatter from a board/local index hash.
                let led = &LED_MAP[i];
                let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % HASH_STEPS as u32;
                let hn = hash as f32 / HASH_STEPS as f32; // 0..1 per-LED
                // Radial gradient + per-LED scatter + the spiral's SPIRAL_ARMS sectors.
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
            rng: 0x6A09_E667,
            glints: Glints::new(0xBB67_AE85, GLINTS),
            last_ms: 0,
        };
        for n in 0..RING_SLOTS {
            s.ring_dark[n] = s.random_dark();
        }
        s
    }

    fn next_rng(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn randf(&mut self) -> f32 {
        (self.next_rng() >> 8) as f32 / 16_777_216.0
    }

    /// A new ring's brightness at its darkest, 0-255.
    fn random_dark(&mut self) -> u8 {
        ((DARK_MIN + self.randf() * (DARK_MAX - DARK_MIN)) * 255.0) as u8
    }

    /// Glide the speed toward its target, picking a new one when the wait is up, then move
    /// the wave on and give each ring that started at the center its own darkness.
    fn advance_wave(&mut self, t_ms: u32, dt_ms: u32) {
        if t_ms.wrapping_sub(self.speed_pick_ms) >= self.speed_gap_ms {
            let m = SPEED_MIN + self.randf() * (SPEED_MAX - SPEED_MIN);
            self.speed_target_q8 = (self.speed * m * 256.0) as i32;
            self.speed_pick_ms = t_ms;
            self.speed_gap_ms = SPEED_CHANGE_MIN_MS
                + (self.randf() * (SPEED_CHANGE_MAX_MS - SPEED_CHANGE_MIN_MS) as f32) as u32;
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

/// HSV -> RGB with integer math. `h6` is the hue in sixths of a turn x4096, `v` is 0-255.
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
    fn render(&mut self, _leds: &[Led], t_ms: u32, out: &mut Frame) {
        let dt_ms = t_ms.wrapping_sub(self.last_ms).min(100);
        self.last_ms = t_ms;
        self.advance_wave(t_ms, dt_ms);

        // Fold each time term to its own period first: raw t_ms loses precision after
        // hours of uptime. Slow hue drift and the pinwheel's rotation, both 0..1 x65536.
        let drift = (t_ms % DRIFT_PERIOD_MS) * 65536 / DRIFT_PERIOD_MS;
        let spin = (t_ms % SPIRAL_PERIOD_MS) * 65536 / SPIRAL_PERIOD_MS;
        let hue_shift = drift.wrapping_sub(spin);
        // How tightly the arms wind right now, swinging between SPIRAL_TWIST one way and the
        // other. Cycles per mm x2^20.
        let twist = (SPIRAL_TWIST
            * ((t_ms % TWIST_PERIOD_MS) as f32 / TWIST_PERIOD_MS as f32 * TAU).sin()
            * 1_048_576.0) as i32;

        for (i, o) in out.iter_mut().enumerate() {
            // Radial wave (the motion), reshaped by WAVE_FALLOFF so the crest-to-trough
            // falloff broadens the bright band and keeps the dark troughs thin. Rings change
            // over at the crest, where every ring is at full brightness, so it never shows.
            let x = self.wave_phase.wrapping_sub(self.ring_pos[i]);
            let wave = self.wave_lut[((x >> (16 - WAVE_BITS)) as usize) & (WAVE_STEPS - 1)] as i32;
            let dark = self.ring_dark[((x >> 16) as usize) & (RING_SLOTS - 1)] as i32;

            // Brightness field, lifted off zero by this ring's darkness (up to 255).
            let v = dark + (((255 - dark) * wave) >> 12);

            // Hue position: the fixed part, plus drift and spin, plus the spiral's twist by
            // radius (pinwheels outward).
            let wind = ((twist * self.dist_q4[i]) >> 8) as u32;
            let p = self.hue_base[i].wrapping_add(hue_shift).wrapping_add(wind);

            // Ping-pong p into a triangle 0->1->0 so the hue sweeps up the cool arc and
            // back down - concentric bands with no seam where p wraps.
            let frac = p & 0xFFFF;
            let tri = if frac < 0x8000 { frac << 1 } else { (0xFFFF - frac) << 1 };
            let hue = HUE_START_Q + ((HUE_SPAN_Q * tri) >> 16);
            *o = hsv_q(hue as i32, v);
        }

        self.glints.update(t_ms, dt_ms, out);
    }
}
