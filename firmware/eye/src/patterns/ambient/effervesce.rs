use crate::audio::Audio;
use crate::led::map::{Led, LED_COUNT, LED_MAP};
use crate::patterns::{
    cycle, hash01, hash2, hsv, random_u32, rate_angle, slot_life, smoothstep, wrap360, Frame,
    HueMix, Pattern, Phasor,
};
use core::f32::consts::{PI, TAU};

// Base field
const HUE_DRIFT_MS: u32 = 40_000; // time for the field hue to go around the wheel
const BASE_SAT: f32 = 0.9;
const BASE_VAL: f32 = 0.4; // field brightness floor

// A slow brightness swell that drifts across the field.
const SWELL: f32 = 0.25; // how much the swell lifts brightness
const SWELL_SPAN_MM: f32 = 260.0; // crest spacing along x or y; crests run diagonally, 184 mm apart
const SWELL_MS: u32 = 12_000;

// Shapes
const SHAPE_COUNT: usize = 7; // shapes alive at once
const LIFETIME_MIN_MS: u32 = 3_500; // each slot keeps its own lifetime in this range
const LIFETIME_MAX_MS: u32 = 8_000;
const WHITEN: f32 = 0.3; // how far a shape goes toward white
const SHAPE_SALT: u32 = 0x5EED; // mixed into each shape slot's hash

const RING_MAX_MM: f32 = 150.0; // how far a ring expands over its life
const RING_THICKNESS_MM: f32 = 40.0; // ring half-width
const BAR_WIDTH_MM: f32 = 26.0; // bar half-width
const BAR_LENGTH_MM: f32 = 130.0; // bar half-length
const SPIN: f32 = 0.5; // turns a bar or star makes over its life
const EDGE_SHARP: f32 = 2.5; // above 1 gives shapes a solid body and a defined edge

// Glitter
const GLITTER: f32 = 0.35; // per-LED glitter strength
const GLITTER_RATE: f32 = 0.004; // flicker rate (rad/ms)

const ROT60: Phasor = Phasor {
    sin: 0.866_025_4,
    cos: 0.5,
};

enum Shape {
    /// An expanding band: its radius and its squared bounds.
    Ring {
        radius: f32,
        d2_min: f32,
        d2_max: f32,
    },
    /// A rotating line through the origin, along a unit direction.
    Bar { axis: Phasor },
    /// Three lines 60 degrees apart.
    Star { axes: [Phasor; 3] },
}

/// One live shape, with its trig worked out once for the frame.
struct LiveShape {
    shape: Shape,
    x: f32, // origin, always an LED position
    y: f32,
    brightness: f32, // for the current age, 0..1
    hue: Phasor,     // cos and sin of the hue angle
}

/// Effervesce: a bright field of slowly drifting hue, with rings, bars and stars appearing
/// at random spots, each in its own color, under a fine glitter. Each slot's shape comes
/// from a hash of (slot, generation) and a seed drawn each time the pattern comes on.
pub struct Effervesce {
    // Each LED's offset in the glitter.
    glitter_offset: [Phasor; LED_COUNT],
    /// New each time the pattern comes on screen, so its shapes differ.
    seed: u32,
}

impl Effervesce {
    pub fn new() -> Self {
        let phase =
            |i: usize| hash01(LED_MAP[i].board_id as u32, LED_MAP[i].local_idx as u32) * TAU;
        Effervesce {
            glitter_offset: core::array::from_fn(|i| Phasor::new(phase(i))),
            seed: 0,
        }
    }
}

impl Pattern for Effervesce {
    fn on_enter(&mut self, _t_ms: u32) {
        self.seed = random_u32();
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let shapes: [LiveShape; SHAPE_COUNT] =
            core::array::from_fn(|slot| live_shape(slot, t_ms, self.seed));

        let base_hue = cycle(t_ms, HUE_DRIFT_MS) * 360.0;
        let base = Phasor::new(base_hue.to_radians());
        let swell_t = cycle(t_ms, SWELL_MS);
        let glitter_phase = Phasor::new(rate_angle(t_ms, GLITTER_RATE));

        for (i, led) in LED_MAP.iter().enumerate() {
            // Base field with the swell, a smoothed triangle wave.
            let mut sat = BASE_SAT;
            let mut val = BASE_VAL + SWELL * swell(led, swell_t) * (1.0 - BASE_VAL);

            // Brightness is the strongest shape; hue is the weighted mix of the shapes and field.
            let mut peak = 0.0f32;
            let mut mix = HueMix::default();
            for shape in &shapes {
                let intensity = shape_intensity(shape, led);
                if intensity > 0.0 {
                    mix.add(intensity, shape.hue);
                    peak = peak.max(intensity);
                }
            }
            mix.add(1.0 - peak, base);
            let hue = mix.degrees();

            val += (1.0 - val) * peak;
            sat *= 1.0 - WHITEN * peak;

            // Glitter, cubed so only the peaks show.
            let wave = self.glitter_offset[i].sin_plus(glitter_phase);
            let cubed = (0.5 + 0.5 * wave).powi(3);
            let glitter = GLITTER * cubed;
            val += (1.0 - val) * glitter;
            sat *= 1.0 - glitter;

            out[i] = hsv(wrap360(hue), sat, val);
        }
    }
}

/// The swell at `led`, from 0 to 1.
fn swell(led: &Led, swell_t: f32) -> f32 {
    let swell_pos = (led.wx + led.wy) / SWELL_SPAN_MM - swell_t;
    let frac = swell_pos - swell_pos.floor();
    let tri = 1.0 - (2.0 * frac - 1.0).abs();
    smoothstep(tri)
}

/// The current shape in `slot`, from time and hashes.
fn live_shape(slot: usize, t_ms: u32, show_seed: u32) -> LiveShape {
    // Each slot has its own lifetime and start offset, so the slots do not respawn together.
    let (shape_num, age) = slot_life(
        slot as u32 + 1,
        SHAPE_SALT,
        t_ms,
        LIFETIME_MIN_MS,
        LIFETIME_MAX_MS,
    );

    let seed = hash2(slot as u32 + 1, shape_num ^ show_seed);
    let led = &LED_MAP[(seed as usize) % LED_COUNT];
    let start_angle = hash01(seed, 1) * TAU;
    let spin_dir = if hash01(seed, 2) < 0.5 { 1.0 } else { -1.0 };
    let hue = hash01(seed, 3) * 360.0;

    let axis = Phasor::new(start_angle + spin_dir * age * SPIN * TAU);
    let brightness = (PI * age).sin(); // fades in, peaks mid-life, fades out

    let shape = match (seed >> 8) % 3 {
        0 => {
            let radius = age * RING_MAX_MM;
            // A ring smaller than its thickness has no hole; -1 is below every squared distance.
            let inner = radius - RING_THICKNESS_MM;
            let d2_min = if inner > 0.0 { inner * inner } else { -1.0 };
            let outer = radius + RING_THICKNESS_MM;
            Shape::Ring {
                radius,
                d2_min,
                d2_max: outer * outer,
            }
        }
        1 => Shape::Bar { axis },
        _ => {
            let second = rot60(axis);
            Shape::Star {
                axes: [axis, second, rot60(second)],
            }
        }
    };

    LiveShape {
        shape,
        x: led.wx,
        y: led.wy,
        brightness,
        hue: Phasor::new(hue.to_radians()),
    }
}

/// One shape's brightness at one LED, in 0..1.
fn shape_intensity(live: &LiveShape, led: &Led) -> f32 {
    let dx = led.wx - live.x;
    let dy = led.wy - live.y;
    match live.shape {
        Shape::Ring {
            radius,
            d2_min,
            d2_max,
        } => {
            let d2 = dx * dx + dy * dy;
            if d2 <= d2_min || d2 >= d2_max {
                return 0.0;
            }
            let d = d2.sqrt();
            let shell =
                ((1.0 - (d - radius).abs() / RING_THICKNESS_MM) * EDGE_SHARP).clamp(0.0, 1.0);
            shell * live.brightness
        }
        Shape::Bar { axis } => line_intensity(dx, dy, axis) * live.brightness,
        // The strongest of the three lines wins.
        Shape::Star { axes } => {
            let [l0, l1, l2] = axes.map(|axis| line_intensity(dx, dy, axis));
            l0.max(l1).max(l2) * live.brightness
        }
    }
}

/// Brightness of a bar through the origin along unit direction `axis`, in 0..1.
fn line_intensity(dx: f32, dy: f32, axis: Phasor) -> f32 {
    let across = (dx * -axis.sin + dy * axis.cos).abs(); // distance from the line
    if across >= BAR_WIDTH_MM {
        return 0.0;
    }
    let along = (dx * axis.cos + dy * axis.sin).abs(); // distance along it from the origin
    let width_fade = ((1.0 - across / BAR_WIDTH_MM) * EDGE_SHARP).clamp(0.0, 1.0);
    let length_fade = (1.0 - along / BAR_LENGTH_MM).max(0.0);
    width_fade * length_fade
}

/// Rotates a unit vector by 60 degrees.
fn rot60(axis: Phasor) -> Phasor {
    Phasor {
        cos: axis.cos * ROT60.cos - axis.sin * ROT60.sin,
        sin: axis.sin * ROT60.cos + axis.cos * ROT60.sin,
    }
}
