use crate::patterns::{Frame, Pattern, hsv, wrap360};
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, WORLD_CX, WORLD_TOP, WORLD_BOT};
use core::f32::consts::TAU;

// ============================ Audition knobs ============================
// Flip these, rebuild, and flash to try each flavor. Everything is independent:
// leave GEOMETRY on Horizontal with every strength at 0.0 for the original plain
// scroll, then turn one - or several - on. Strengths run 0.0 (off) .. 1.0 (full).

/// How the hue is laid across the fixture.
#[allow(dead_code)] // variants are selected by editing GEOMETRY below
#[derive(Clone, Copy, PartialEq)]
pub enum Geometry {
    /// Original left-to-right scroll.
    Horizontal,
    /// Hue follows the angle around the centroid, so the whole rainbow rotates
    /// like a color wheel.
    Spin,
    /// Each of the three vertices owns a hue 120 deg apart; every LED blends
    /// toward its nearest vertices and the trio rotates - color pools at the
    /// points and flows between them.
    Orbit,
}

const GEOMETRY: Geometry = Geometry::Spin;

const TWINKLE:     f32 = 0.7; // swirl of fading around each triangle tile     (try 0.4)
const SPARKLE:     f32 = 0.0; // per-LED independent flicker - a true twinkle  (try 0.35)
const BREATHE:     f32 = 0.5; // scroll/rotation eases like a tide             (try 0.5)
const PASTEL:      f32 = 0.0; // saturation blooms toward white                (try 0.5)
const IRIDESCENCE: f32 = 0.2; // drifting hue ripple - oil-on-water shimmer    (try 0.5)

// ---- Supporting tunables (the feel of each effect; usually leave as-is) ----

// Horizontal distance (mm) over which the hue wheel completes one full cycle (~world width).
const HUE_SPAN_MM: f32 = 517.0;

// Spin / Orbit: time for one full rotation of the wheel / of the vertex-hue trio.
const SPIN_PERIOD_MS:  u32 = 20_000;
const ORBIT_PERIOD_MS: u32 = 24_000;

// Breathe: depth of the speed wobble as a fraction of a hue cycle, over this period.
const BREATHE_DEPTH:     f32 = 0.15;
const BREATHE_PERIOD_MS: u32 = 18_000;

// Twinkle: swirl rate (rad/ms) of the fade that circulates each tile; ~2.4 s per cycle.
const TWINKLE_RATE: f32 = 0.0026;

// Sparkle: per-LED flicker rate (rad/ms). Faster reads as more twinkly; ~1.6 s base cycle.
const SPARKLE_RATE: f32 = 0.004;

// Iridescence: a concentric hue ripple that drifts outward and beats against the base
// rainbow. SPAN is the ring spacing (mm), DEG the max hue swing, PERIOD the drift time.
const IRID_SPAN_MM:   f32 = 70.0;
const IRID_DEG:       f32 = 55.0;
const IRID_PERIOD_MS: u32 = 9_000;

// Pastel: saturation blooms travel across the width on this period; SPAN is the
// distance (mm) between successive blooms.
const PASTEL_PERIOD_MS: u32 = 15_000;
const PASTEL_SPAN_MM:   f32 = 300.0;

// Orbit vertices: approximate corners of the point-down triangle (two top, one apex).
const V_LEFT:  (f32, f32) = (10.0,                  WORLD_TOP);
const V_RIGHT: (f32, f32) = (2.0 * WORLD_CX - 10.0, WORLD_TOP);
const V_APEX:  (f32, f32) = (WORLD_CX,              WORLD_BOT);

pub struct RainbowX {
    pub speed: f32, // mm/s scroll rate (Horizontal); rotation uses the *_PERIOD_MS consts
}

impl Pattern for RainbowX {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        // Breathe: a bounded sinusoidal wobble added to the scroll/rotation phase so the
        // motion visibly eases faster then slower. Kept additive (not a speed multiply) so
        // the steady term below still folds cleanly to its period. Unit: hue cycles.
        let breathe = if BREATHE > 0.0 {
            let ph = (t_ms % BREATHE_PERIOD_MS) as f32 / BREATHE_PERIOD_MS as f32 * TAU;
            BREATHE * BREATHE_DEPTH * ph.sin()
        } else {
            0.0
        };

        // Steady animation term per geometry. Fold time to the term's period before the f32
        // cast: raw t_ms loses sub-frame precision after hours of uptime, and each term is
        // periodic so the fold is seamless.
        let anim = match GEOMETRY {
            Geometry::Horizontal => {
                let period_ms = (HUE_SPAN_MM / self.speed * 1000.0) as u32;
                let steady = ((t_ms % period_ms.max(1)) as f32 / 1000.0) * self.speed;
                steady + breathe * HUE_SPAN_MM // wobble is a fraction of one hue cycle, in mm
            }
            Geometry::Spin => {
                let steady = (t_ms % SPIN_PERIOD_MS) as f32 / SPIN_PERIOD_MS as f32;
                steady + breathe // wobble is already in cycles
            }
            Geometry::Orbit => {
                let steady = (t_ms % ORBIT_PERIOD_MS) as f32 / ORBIT_PERIOD_MS as f32;
                steady + breathe
            }
        };

        // Twinkle time, folded to one swirl period (rounded; the seam is sub-degree).
        let twinkle_t = if TWINKLE > 0.0 {
            let period = (TAU / TWINKLE_RATE) as u32;
            (t_ms % period.max(1)) as f32
        } else {
            0.0
        };

        // Sparkle time, folded to one base flicker period (the 2x harmonic stays periodic in it).
        let sparkle_t = if SPARKLE > 0.0 {
            let period = (TAU / SPARKLE_RATE) as u32;
            (t_ms % period.max(1)) as f32
        } else {
            0.0
        };

        // Iridescence ripple phase, drifting the concentric rings outward.
        let irid_ph = (t_ms % IRID_PERIOD_MS) as f32 / IRID_PERIOD_MS as f32 * TAU;

        // Pastel bloom phase, scrolling the saturation bands across the width.
        let pastel_ph = (t_ms % PASTEL_PERIOD_MS) as f32 / PASTEL_PERIOD_MS as f32;

        for (i, led) in leds.iter().enumerate() {
            // Base hue, optionally warped by a concentric hue ripple on a different axis and
            // frequency than the base, so the two beat into oil-on-water iridescence.
            let mut hue = hue_for(led, i, anim);
            if IRIDESCENCE > 0.0 {
                let r = DIST_C[i];
                let ripple = (r / IRID_SPAN_MM * TAU - irid_ph).sin(); // -1..1
                hue += IRIDESCENCE * IRID_DEG * ripple;
            }

            // Saturation: full, dipped toward white inside slow pastel bloom bands. The bloom
            // is peaky (squared) so the field stays saturated between tight soft-white patches.
            let mut s = 1.0;
            if PASTEL > 0.0 {
                let p = led.wx / PASTEL_SPAN_MM - pastel_ph;
                let bloom = (0.5 - 0.5 * (p * TAU).cos()).powi(2); // 0..1
                s = 1.0 - PASTEL * bloom;
            }

            // Value: full, then dimmed by the two brightness effects (both stay <= 1.0, so
            // peaks keep full brightness).
            let mut v = 1.0;

            // Twinkle: a fade that circulates each tile (phase ramps with local_idx).
            if TWINKLE > 0.0 {
                let hash = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
                let dip = 0.5 - 0.5 * (twinkle_t * TWINKLE_RATE + hash as f32).sin(); // 0..1
                v *= 1.0 - TWINKLE * dip;
            }

            // Sparkle: independent per-LED flicker. A scrambled hash decorrelates neighbors,
            // and two harmonics give each LED its own irregular rhythm so it reads as twinkle.
            if SPARKLE > 0.0 {
                let h = hash32(led.board_id, led.local_idx);
                let p1 = (h & 4095) as f32 / 4096.0 * TAU;
                let p2 = ((h >> 12) & 4095) as f32 / 4096.0 * TAU;
                let flick = 0.6 * (sparkle_t * SPARKLE_RATE + p1).sin()
                          + 0.4 * (sparkle_t * SPARKLE_RATE * 2.0 + p2).sin(); // -1..1
                let dip = 0.5 - 0.5 * flick; // 0..1
                v *= 1.0 - SPARKLE * dip;
            }

            out[i] = hsv(wrap360(hue), s, v);
        }
    }
}

/// Hue in degrees (unwrapped) for one LED, given the geometry's animation term.
#[inline]
fn hue_for(led: &Led, i: usize, anim: f32) -> f32 {
    match GEOMETRY {
        Geometry::Horizontal => (led.wx + anim) / HUE_SPAN_MM * 360.0,
        Geometry::Spin => (THETA_C[i] / TAU + anim) * 360.0,
        Geometry::Orbit => orbit_hue(led, anim),
    }
}

/// Orbit hue: blend the three vertex hues by inverse-square proximity so color pools at
/// each point and melts between them. `rot_rev` rotates all three hues together (in cycles).
fn orbit_hue(led: &Led, rot_rev: f32) -> f32 {
    const VERTS: [(f32, f32); 3] = [V_LEFT, V_RIGHT, V_APEX];
    let base = rot_rev * TAU;
    let (mut vx, mut vy) = (0.0_f32, 0.0_f32);
    for (k, &(px, py)) in VERTS.iter().enumerate() {
        let d2 = (led.wx - px).powi(2) + (led.wy - py).powi(2);
        let w = 1.0 / d2.max(1.0);             // inverse-square: tight pooling at the points
        let h = base + k as f32 * (TAU / 3.0); // three hues 120 deg apart
        vx += w * h.cos();
        vy += w * h.sin();
    }
    vy.atan2(vx) / TAU * 360.0
}

/// Bit-mix hash of an LED's (board, local) index into a scrambled u32, so neighboring LEDs
/// land on unrelated values - used to give each LED an independent sparkle phase.
fn hash32(board: u8, local: u8) -> u32 {
    let mut h = (board as u32).wrapping_mul(0x9E37_79B1) ^ (local as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 13;
    h
}
