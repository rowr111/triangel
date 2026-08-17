use crate::patterns::{Frame, Pattern, hsv, wrap360};
use crate::led::map::Led;
use core::f32::consts::{PI, TAU};

// Effervesce - a bright, slowly hue-drifting color field with geometric elements (expanding
// rings, rotating bars, radial stars) spawning at random spots across the fixture, each its
// own color and speed, blazing bright and fading, all under a fine per-LED glitter.
//
// The elements are a stateless emitter: at any t_ms each of ELEMENTS "slots" derives its
// current element (origin, shape, age) from a hash of (slot, generation), so there is no
// mutable state and no RNG. Lowering ELEMENTS and BASE_VAL toward 0 turns this into a
// sparse "blooms on black" look - the same engine, a darker skin.
//
// Perf note: the target (riscv32imac) has no hardware float, so sin/cos/atan2/sqrt are
// software-emulated and costly. All per-element trig (rotation, envelope) is precomputed
// once per frame in make_elem; the per-LED hot loop stays arithmetic + one ring sqrt.

// ============================== Tuning knobs ==============================

// ---- Base field ----
const HUE_DRIFT_MS: u32 = 40_000; // time for the field hue to travel the whole wheel
const BASE_SAT:     f32 = 0.9;    // field color saturation (higher = richer, so white shapes pop)
const BASE_VAL:     f32 = 0.4;    // field brightness floor (lower = shapes pop harder against it)

// Gentle drifting brightness swell, for depth (0 = flat field). Cheap triangle wave.
const SWELL:         f32 = 0.25;  // how much the swell lifts brightness
const SWELL_SPAN_MM: f32 = 260.0; // distance between swell crests
const SWELL_MS:      u32 = 12_000;

// ---- Emitter ----
const ELEMENTS:      usize = 7;     // geometric elements alive at once (density)
const LIFETIME_MIN_MS: u32 = 3_500; // fastest element (each slot picks its own -> mixed speeds)
const LIFETIME_MAX_MS: u32 = 8_000; // slowest element
const WHITEN:        f32   = 0.3;   // how white the core goes (low = shapes keep their own color)

// ---- Shapes (assortment, cycled per spawn: 0 ring, 1 bar, 2 star) ----
const RING_MAX_MM:       f32 = 150.0; // how far a bubble expands over its life
const RING_THICKNESS_MM: f32 = 40.0;  // shell band width (bold enough to light several LEDs)
const BAR_WIDTH_MM:      f32 = 26.0;  // line half-width (bold enough to catch several LEDs)
const BAR_LENGTH_MM:     f32 = 130.0; // line half-length
const SPIN:              f32 = 0.5;   // rotations a bar/star turns over its life
const EDGE_SHARP:        f32 = 2.5;   // >1 gives shapes a solid body + defined edge (distinctness)

// ---- Glitter ----
const SHIMMER:      f32 = 0.35;  // per-LED sparkle strength
const SHIMMER_RATE: f32 = 0.004; // flicker rate (rad/ms)

// 60-degree rotation constants, for building a 3-line star without trig in the hot loop.
const COS60: f32 = 0.5;
const SIN60: f32 = 0.866_025_4;

// One live geometric element, with all its trig precomputed for the frame.
#[derive(Clone, Copy)]
struct Elem {
    kind: u8,  // 0 ring, 1 bar, 2 star
    ox:   f32, // origin (snapped to a real LED position, so it's always on the fixture)
    oy:   f32,
    env:  f32, // brightness envelope for the current age (0..1)
    r:    f32, // ring radius (ring only)
    cs:   f32, // primary direction cosine (bar / star)
    sn:   f32, // primary direction sine
    chue: f32, // this element's color as a wheel vector (cos), for continuous hue blending
    shue: f32, // ...and its sin
}

pub struct Effervesce;

impl Pattern for Effervesce {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        // Precompute the live elements once per frame (derived purely from t_ms + hashes).
        let elems: [Elem; ELEMENTS] = core::array::from_fn(|s| make_elem(s, t_ms, leds));

        // Field hue drifts slowly through the wheel; kept as a wheel vector so it blends
        // continuously with the element colors below.
        let base_hue = (t_ms % HUE_DRIFT_MS) as f32 / HUE_DRIFT_MS as f32 * 360.0;
        let (base_shue, base_chue) = base_hue.to_radians().sin_cos();

        // Drifting brightness swell (scroll offset, 0..1).
        let swell_t = (t_ms % SWELL_MS) as f32 / SWELL_MS as f32;

        // Glitter time, folded to one flicker period (rounded; the seam is sub-degree).
        let shimmer_t = {
            let period = (TAU / SHIMMER_RATE) as u32;
            (t_ms % period.max(1)) as f32
        };

        for (i, led) in leds.iter().enumerate() {
            // Base field: a bright, saturated wash of the drifting hue, with a gentle swell.
            // Triangle wave (no trig): a smooth hump scrolling across the fixture.
            let p = (led.wx + led.wy) / SWELL_SPAN_MM - swell_t;
            let frac = p - p.floor();
            let tri = 1.0 - (2.0 * frac - 1.0).abs();
            let sw = tri * tri * (3.0 - 2.0 * tri); // smoothstep the triangle
            let mut s = BASE_SAT;
            let mut v = BASE_VAL + SWELL * sw * (1.0 - BASE_VAL);

            // Elements: brightness is the strongest shape's coverage (max, which stays
            // continuous). Color is a weighted average of the overlapping shapes' colors taken as
            // vectors on the wheel (plus the base field where they don't cover), so overlaps mix
            // smoothly with no hue snap when one shape overtakes another.
            let mut e = 0.0f32;
            let mut vx = 0.0f32;
            let mut vy = 0.0f32;
            for elem in &elems {
                let c = shape_intensity(elem, led);
                vx += c * elem.chue;
                vy += c * elem.shue;
                e = e.max(c);
            }
            // Fill the remaining weight with the base field color, then read off the wheel angle.
            vx += (1.0 - e) * base_chue;
            vy += (1.0 - e) * base_shue;
            let hue = vy.atan2(vx).to_degrees();

            // The shapes brighten above the field; a low WHITEN keeps them colored, not white.
            v += (1.0 - v) * e;
            s *= 1.0 - WHITEN * e;

            // Glitter: sparse per-LED sparkle (cubed so it reads as discrete twinkles).
            let hh = hash2(led.board_id as u32, led.local_idx as u32);
            let ph = (hh & 4095) as f32 / 4096.0 * TAU;
            let spark = (0.5 + 0.5 * (shimmer_t * SHIMMER_RATE + ph).sin()).powi(3);
            let g = SHIMMER * spark;
            v += (1.0 - v) * g;
            s *= 1.0 - g;

            out[i] = hsv(wrap360(hue), s, v);
        }
    }
}

/// Derive slot `s`'s current element from time and hashes (no mutable state). All the trig
/// lives here (ELEMENTS times per frame), keeping the per-LED loop arithmetic-only.
fn make_elem(s: usize, t_ms: u32, leds: &[Led]) -> Elem {
    // Per-slot random speed: each slot keeps its own fixed lifetime, so fast and slow shapes
    // coexist. A random phase offset keeps the slots from popping in sync.
    let slot_seed = hash2(s as u32 + 1, 0x5EED);
    let life  = LIFETIME_MIN_MS + slot_seed % (LIFETIME_MAX_MS - LIFETIME_MIN_MS);
    let local = t_ms.wrapping_add(slot_seed % life);
    let gen   = local / life;
    let age   = (local % life) as f32 / life as f32;

    // New hash each generation -> a fresh origin/shape/angle/color when the slot respawns.
    let seed = hash2(s as u32 + 1, gen);
    let led  = &leds[(seed as usize) % leds.len()];
    let kind = ((seed >> 8) % 3) as u8;
    let ang0 = ((seed >> 12) & 0xFFFF) as f32 / 65_536.0 * TAU;
    let dir  = if (seed >> 28) & 1 == 0 { 1.0 } else { -1.0 };
    let hue  = (seed & 0x1FF) as f32 / 512.0 * 360.0;

    // Precompute rotation, envelope, and the color as a wheel vector for this age.
    let (sn, cs) = (ang0 + dir * age * SPIN * TAU).sin_cos();
    let (shue, chue) = hue.to_radians().sin_cos();
    let env = (PI * age).sin(); // every shape fades in at birth, peaks mid-life, fades out
    let r = age * RING_MAX_MM;

    Elem { kind, ox: led.wx, oy: led.wy, env, r, cs, sn, chue, shue }
}

/// One element's brightness contribution at one LED, in 0..1. Arithmetic-only except the
/// ring's single sqrt.
fn shape_intensity(e: &Elem, led: &Led) -> f32 {
    let dx = led.wx - e.ox;
    let dy = led.wy - e.oy;
    match e.kind {
        // Ring (bubble): a thin shell expanding outward, brightest at birth, fading as it grows.
        0 => {
            let d = (dx * dx + dy * dy).sqrt();
            let shell = ((1.0 - (d - e.r).abs() / RING_THICKNESS_MM) * EDGE_SHARP).clamp(0.0, 1.0);
            shell * e.env
        }
        // Bar: a bright line through the origin, rotating over its life; fades in and out.
        1 => line_intensity(dx, dy, e.cs, e.sn) * e.env,
        // Star: three lines at 60 degrees (six rays), the strongest wins.
        _ => {
            let l0 = line_intensity(dx, dy, e.cs, e.sn);
            let (c1, s1) = rot60(e.cs, e.sn);
            let (c2, s2) = rot60(c1, s1);
            let l1 = line_intensity(dx, dy, c1, s1);
            let l2 = line_intensity(dx, dy, c2, s2);
            l0.max(l1).max(l2) * e.env
        }
    }
}

/// Brightness of a bright bar through the origin along unit direction (cs, sn), in 0..1.
fn line_intensity(dx: f32, dy: f32, cs: f32, sn: f32) -> f32 {
    let perp  = (dx * -sn + dy * cs).abs(); // distance from the line
    let along = (dx * cs + dy * sn).abs();  // distance along it from the origin
    let w = ((1.0 - perp / BAR_WIDTH_MM) * EDGE_SHARP).clamp(0.0, 1.0); // plateau -> solid body, defined edge
    let l = (1.0 - along / BAR_LENGTH_MM).max(0.0);
    w * l
}

/// Rotate a unit vector by +60 degrees (constant-coefficient, no trig).
fn rot60(c: f32, s: f32) -> (f32, f32) {
    (c * COS60 - s * SIN60, s * COS60 + c * SIN60)
}

/// Bit-mix hash of two u32s into a scrambled u32.
fn hash2(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 13;
    h
}
