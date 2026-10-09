use crate::audio::Audio;
use crate::patterns::{Frame, Pattern, cycle, fold_ms, hash2, hsv, sin_sum, smoothstep, wrap360};
use crate::led::map::{Led, LED_COUNT, LED_MAP};
use core::f32::consts::{PI, TAU};

// Effervesce: a bright field of slowly drifting hue, with rings, bars and stars appearing
// at random spots, each in its own color, under a fine glitter. Each slot's shape comes
// from a hash of (slot, generation), so the pattern keeps no state.

// Base field
const HUE_DRIFT_MS: u32 = 40_000; // time for the field hue to go around the wheel
const BASE_SAT:     f32 = 0.9;
const BASE_VAL:     f32 = 0.4;    // field brightness floor

// A slow brightness swell that drifts across the field.
const SWELL:         f32 = 0.25;  // how much the swell lifts brightness
const SWELL_SPAN_MM: f32 = 260.0; // distance between crests
const SWELL_MS:      u32 = 12_000;

// Shapes
const ELEMENTS:      usize = 7;     // shapes alive at once
const LIFETIME_MIN_MS: u32 = 3_500; // each slot keeps its own lifetime in this range
const LIFETIME_MAX_MS: u32 = 8_000;
const WHITEN:        f32   = 0.3;   // how far a shape goes toward white

const RING_MAX_MM:       f32 = 150.0; // how far a ring expands over its life
const RING_THICKNESS_MM: f32 = 40.0;  // ring half-width
const BAR_WIDTH_MM:      f32 = 26.0;  // bar half-width
const BAR_LENGTH_MM:     f32 = 130.0; // bar half-length
const SPIN:              f32 = 0.5;   // turns a bar or star makes over its life
const EDGE_SHARP:        f32 = 2.5;   // above 1 gives shapes a solid body and a defined edge

// Glitter
const SHIMMER:      f32 = 0.35;  // per-LED sparkle strength
const SHIMMER_RATE: f32 = 0.004; // flicker rate (rad/ms)

const COS60: f32 = 0.5;
const SIN60: f32 = 0.866_025_4;

// One live shape, with its trig worked out once for the frame.
#[derive(Clone, Copy)]
struct Elem {
    kind: u8,  // 0 ring, 1 bar, 2 star
    ox:   f32, // origin, always an LED position
    oy:   f32,
    env:  f32, // brightness for the current age, 0..1
    r:    f32, // ring radius
    cs:   f32, // direction of the bar, or of the star's first line
    sn:   f32,
    chue: f32, // cos and sin of the hue angle
    shue: f32,
    dirs:   [(f32, f32); 2], // the star's other two lines
    d2_min: f32,             // squared bounds of the ring
    d2_max: f32,
}

pub struct Effervesce {
    // sin and cos of each LED's glitter phase.
    ph_sin: [f32; LED_COUNT],
    ph_cos: [f32; LED_COUNT],
}

impl Effervesce {
    pub fn new() -> Self {
        let phase = |i: usize| {
            let hh = hash2(LED_MAP[i].board_id as u32, LED_MAP[i].local_idx as u32);
            (hh & 4095) as f32 / 4096.0 * TAU
        };
        Effervesce {
            ph_sin: core::array::from_fn(|i| phase(i).sin()),
            ph_cos: core::array::from_fn(|i| phase(i).cos()),
        }
    }
}

impl Pattern for Effervesce {
    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let elems: [Elem; ELEMENTS] = core::array::from_fn(|s| make_elem(s, t_ms));

        let base_hue = cycle(t_ms, HUE_DRIFT_MS) * 360.0;
        let (base_shue, base_chue) = base_hue.to_radians().sin_cos();
        let swell_t = cycle(t_ms, SWELL_MS);
        let shimmer_t = fold_ms(t_ms, SHIMMER_RATE);
        let (sh_sin, sh_cos) = (shimmer_t * SHIMMER_RATE).sin_cos();

        for (i, led) in LED_MAP.iter().enumerate() {
            // Base field with the swell, a smoothed triangle wave.
            let p = (led.wx + led.wy) / SWELL_SPAN_MM - swell_t;
            let frac = p - p.floor();
            let tri = 1.0 - (2.0 * frac - 1.0).abs();
            let sw = smoothstep(tri);
            let mut s = BASE_SAT;
            let mut v = BASE_VAL + SWELL * sw * (1.0 - BASE_VAL);

            // Brightness is the strongest shape here. Hue is the weighted average of the
            // overlapping shapes and the field, as vectors on the color wheel, so it never jumps.
            let mut e = 0.0f32;
            let mut vx = 0.0f32;
            let mut vy = 0.0f32;
            for elem in &elems {
                let c = shape_intensity(elem, led);
                if c > 0.0 {
                    vx += c * elem.chue;
                    vy += c * elem.shue;
                    e = e.max(c);
                }
            }
            vx += (1.0 - e) * base_chue;
            vy += (1.0 - e) * base_shue;
            let hue = vy.atan2(vx).to_degrees();

            v += (1.0 - v) * e;
            s *= 1.0 - WHITEN * e;

            // Glitter, cubed so only the peaks show.
            let sh = sin_sum(self.ph_sin[i], self.ph_cos[i], sh_sin, sh_cos);
            let spark = (0.5 + 0.5 * sh).powi(3);
            let g = SHIMMER * spark;
            v += (1.0 - v) * g;
            s *= 1.0 - g;

            out[i] = hsv(wrap360(hue), s, v);
        }
    }
}

/// Slot `s`'s current shape, from time and hashes.
fn make_elem(s: usize, t_ms: u32) -> Elem {
    // Each slot has its own lifetime and start offset, so the slots do not respawn together.
    let slot_seed = hash2(s as u32 + 1, 0x5EED);
    let life  = LIFETIME_MIN_MS + slot_seed % (LIFETIME_MAX_MS - LIFETIME_MIN_MS);
    let local = t_ms.wrapping_add(slot_seed % life);
    let gen   = local / life;
    let age   = (local % life) as f32 / life as f32;

    let seed = hash2(s as u32 + 1, gen);
    let led  = &LED_MAP[(seed as usize) % LED_COUNT];
    let kind = ((seed >> 8) % 3) as u8;
    let ang0 = ((seed >> 12) & 0xFFFF) as f32 / 65_536.0 * TAU;
    let dir  = if (seed >> 28) & 1 == 0 { 1.0 } else { -1.0 };
    let hue  = (seed & 0x1FF) as f32 / 512.0 * 360.0;

    let (sn, cs) = (ang0 + dir * age * SPIN * TAU).sin_cos();
    let (shue, chue) = hue.to_radians().sin_cos();
    let env = (PI * age).sin(); // fades in, peaks mid-life, fades out
    let r = age * RING_MAX_MM;

    let d1 = rot60(cs, sn);
    let dirs = [d1, rot60(d1.0, d1.1)];
    // A ring smaller than its thickness has no inner edge; -1 never matches a squared distance.
    let inner = r - RING_THICKNESS_MM;
    let d2_min = if inner > 0.0 { inner * inner } else { -1.0 };
    let outer = r + RING_THICKNESS_MM;

    Elem {
        kind,
        ox: led.wx,
        oy: led.wy,
        env,
        r,
        cs,
        sn,
        chue,
        shue,
        dirs,
        d2_min,
        d2_max: outer * outer,
    }
}

/// One shape's brightness at one LED, in 0..1.
fn shape_intensity(e: &Elem, led: &Led) -> f32 {
    let dx = led.wx - e.ox;
    let dy = led.wy - e.oy;
    match e.kind {
        // Ring: an expanding band.
        0 => {
            let d2 = dx * dx + dy * dy;
            if d2 <= e.d2_min || d2 >= e.d2_max {
                return 0.0;
            }
            let d = d2.sqrt();
            let shell = ((1.0 - (d - e.r).abs() / RING_THICKNESS_MM) * EDGE_SHARP).clamp(0.0, 1.0);
            shell * e.env
        }
        // Bar: a rotating line through the origin.
        1 => line_intensity(dx, dy, e.cs, e.sn) * e.env,
        // Star: three lines 60 degrees apart, the strongest wins.
        _ => {
            let l0 = line_intensity(dx, dy, e.cs, e.sn);
            let l1 = line_intensity(dx, dy, e.dirs[0].0, e.dirs[0].1);
            let l2 = line_intensity(dx, dy, e.dirs[1].0, e.dirs[1].1);
            l0.max(l1).max(l2) * e.env
        }
    }
}

/// Brightness of a bar through the origin along unit direction (cs, sn), in 0..1.
fn line_intensity(dx: f32, dy: f32, cs: f32, sn: f32) -> f32 {
    let perp  = (dx * -sn + dy * cs).abs(); // distance from the line
    if perp >= BAR_WIDTH_MM {
        return 0.0;
    }
    let along = (dx * cs + dy * sn).abs();  // distance along it from the origin
    let w = ((1.0 - perp / BAR_WIDTH_MM) * EDGE_SHARP).clamp(0.0, 1.0);
    let l = (1.0 - along / BAR_LENGTH_MM).max(0.0);
    w * l
}

/// Rotates a unit vector by 60 degrees.
fn rot60(c: f32, s: f32) -> (f32, f32) {
    (c * COS60 - s * SIN60, s * COS60 + c * SIN60)
}
