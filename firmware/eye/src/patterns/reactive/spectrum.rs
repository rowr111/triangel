use core::f32::consts::TAU;

use crate::audio::{Audio, MEL_BANDS};
use crate::led::geom::{DIST_C, THETA_C};
use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, ReactivePattern, lerp};

// Color runs as a ramp of RGB stops from the core to the rim, rather than a sweep of
// hue. A ramp can leave the color wheel - passing through white, or desaturating in the
// middle - which is what gives a fire its shape and a hue sweep cannot do.
//
// Every stop is scaled so its brightest channel is 255, leaving brightness entirely to
// `v`. A stop that peaked lower would cap that part of the ramp below full.
const STOPS: usize = 5;
const PALETTES: [[[f32; 3]; STOPS]; 6] = [
    // Ember: red through orange and gold to a blue-white core.
    [[255.0, 28.0, 0.0], [255.0, 90.0, 0.0], [255.0, 190.0, 40.0], [255.0, 240.0, 200.0],
     [170.0, 210.0, 255.0]],
    // Cool arc: cyan up through blue and violet to magenta.
    [[0.0, 255.0, 255.0], [0.0, 150.0, 255.0], [90.0, 90.0, 255.0], [180.0, 70.0, 255.0],
     [255.0, 70.0, 205.0]],
    // Aurora: green through teal and cyan to white.
    [[60.0, 255.0, 120.0], [0.0, 255.0, 195.0], [0.0, 233.0, 255.0], [150.0, 215.0, 255.0],
     [240.0, 250.0, 255.0]],
    // Sunset: magenta through rose and orange to cream.
    [[255.0, 45.0, 140.0], [255.0, 85.0, 95.0], [255.0, 145.0, 45.0], [255.0, 205.0, 65.0],
     [255.0, 240.0, 195.0]],
    // Teal to rose, easing through a warm cream so the two never meet head on.
    [[0.0, 255.0, 248.0], [128.0, 255.0, 238.0], [255.0, 245.0, 198.0], [255.0, 170.0, 80.0],
     [255.0, 110.0, 140.0]],
    // One hue from deep to white, for a controlled look among the busier ramps.
    [[146.0, 55.0, 255.0], [163.0, 76.0, 255.0], [190.0, 110.0, 255.0], [225.0, 180.0, 255.0],
     [250.0, 240.0, 255.0]],
];
/// Time on each palette, and how much of that is spent fading into the next.
const PALETTE_MS: u32 = 25_000;
const PALETTE_FADE_MS: u32 = 5_000;
/// How far the ripple pushes an LED along the ramp, which keeps the colors moving
/// without anything rotating through hues the palette does not contain.
const RIPPLE_SHIFT: f32 = 0.10;
/// Maps a band index onto the ramp.
const BAND_TO_RAMP: f32 = 1.0 / (MEL_BANDS - 1) as f32;

// Floor under the loudness scale, so a quiet room still shows the band shape.
const QUIET_FLOOR: f32 = 0.20;

// Steepens the brightness ramp. Kept below the point where a sustained bass line
// clips solid, so a kick still has somewhere to go above it.
const GAIN: f32 = 1.25;

// A band's onset adds this much brightness and whitens it by this much. The onset is
// what makes a kick read, so it carries more of the range than the steady level does.
const HIT_GAIN:  f32 = 0.9;
const HIT_WHITE: f32 = 0.65;

// Slow angular ripple, so the rings breathe instead of sitting still.
const LOBES:            f32 = 3.0;
const SWIRL_DEPTH:      f32 = 0.18;
const SWIRL_PERIOD_MS:  u32 = 12_000;

/// The 24 mel bands mapped onto distance from the fixture centroid: bass at the core,
/// treble at the rim. Bands are assigned by radius rank rather than by radius, so each
/// one drives the same number of LEDs - by radius the outermost bands would land on
/// six LEDs apiece and the treble would not read at all.
pub struct Spectrum {
    /// Fractional band index per LED, 0.0 to MEL_BANDS-1, for interpolating neighbors.
    band_pos: [f32; LED_COUNT],
    /// sin and cos of LOBES * theta, so the ripple rotates without a per-LED sin call.
    swirl_s: [f32; LED_COUNT],
    swirl_c: [f32; LED_COUNT],
}

impl Spectrum {
    pub fn new() -> Self {
        let mut order: [usize; LED_COUNT] = core::array::from_fn(|i| i);
        order.sort_unstable_by(|&a, &b| {
            DIST_C[a].partial_cmp(&DIST_C[b]).unwrap_or(core::cmp::Ordering::Equal)
        });
        let mut band_pos = [0.0f32; LED_COUNT];
        let span = (MEL_BANDS - 1) as f32 / LED_COUNT as f32;
        for (rank, &i) in order.iter().enumerate() {
            band_pos[i] = rank as f32 * span;
        }
        Spectrum {
            band_pos,
            swirl_s: core::array::from_fn(|i| (LOBES * THETA_C[i]).sin()),
            swirl_c: core::array::from_fn(|i| (LOBES * THETA_C[i]).cos()),
        }
    }
}

impl ReactivePattern for Spectrum {
    fn render(&mut self, _leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame) {
        // Loudness relative to recent music, not absolute: heavily compressed tracks
        // barely move the absolute level, so it would leave brightness nearly flat.
        let loud = QUIET_FLOOR + (1.0 - QUIET_FLOOR) * audio.level_norm;
        // Rotate the ripple once per period. Wrapping the clock first keeps f32 exact.
        let w = (t_ms % SWIRL_PERIOD_MS) as f32 / SWIRL_PERIOD_MS as f32 * TAU;
        let (ws, wc) = w.sin_cos();
        // Cross-fade the palettes once per frame rather than per LED.
        let slot = (t_ms / PALETTE_MS) as usize % PALETTES.len();
        let within = t_ms % PALETTE_MS;
        let held = PALETTE_MS - PALETTE_FADE_MS;
        let mix = if within > held {
            (within - held) as f32 / PALETTE_FADE_MS as f32
        } else {
            0.0
        };
        let (from, to) = (&PALETTES[slot], &PALETTES[(slot + 1) % PALETTES.len()]);
        let mut ramp = [[0.0f32; 3]; STOPS];
        for (stop, (a, b)) in ramp.iter_mut().zip(from.iter().zip(to.iter())) {
            for (c, (x, y)) in stop.iter_mut().zip(a.iter().zip(b.iter())) {
                *c = lerp(*x, *y, mix);
            }
        }

        for (i, o) in out.iter_mut().enumerate() {
            let pos = self.band_pos[i];
            let k = pos as usize;
            let f = pos - k as f32;
            let energy = lerp(audio.bands[k], audio.bands[k + 1], f);
            let hit = lerp(audio.rise[k], audio.rise[k + 1], f);

            // One ripple value drives both the brightness and the color swirl. Being a
            // sinusoid of the angle, it meets itself at the wrap ray with no seam.
            let ripple = self.swirl_s[i] * wc + self.swirl_c[i] * ws;
            let lit = energy * loud * GAIN * (1.0 + SWIRL_DEPTH * ripple);
            let v = (lit + hit * HIT_GAIN).clamp(0.0, 1.0);

            // Position along the ramp, nudged by the ripple so the colors keep moving.
            let t = (pos * BAND_TO_RAMP + RIPPLE_SHIFT * ripple).clamp(0.0, 1.0)
                * (STOPS - 1) as f32;
            let seg = (t as usize).min(STOPS - 2);
            let u = t - seg as f32;
            let white = hit * HIT_WHITE;
            for (ch, o_ch) in o.iter_mut().enumerate() {
                let c = lerp(ramp[seg][ch], ramp[seg + 1][ch], u);
                *o_ch = (lerp(c, 255.0, white) * v) as u8;
            }
        }
    }
}
