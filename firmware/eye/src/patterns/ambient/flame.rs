use std::sync::OnceLock;

use crate::led::geom::DIST_APEX;
use crate::patterns::{Frame, Pattern, lerp, phase_phasors, PHASE_STEPS};
use crate::led::map::{Led, WORLD_BOT, WORLD_CX, WORLD_H, LED_COUNT};
use core::f32::consts::{PI, TAU};

// Tunables - dial these in the previewer. Each scales one ingredient of the flame;
// pushing one toward zero removes that ingredient.
const SECOND_WAVE_STRENGTH:    f32 = 0.5;  // interference wave amplitude vs the primary
const SECOND_WAVELENGTH_RATIO: f32 = 1.7;  // second wave's wavelength vs primary (incommensurate)
const SECOND_SPEED_RATIO:      f32 = 0.63; // second wave's speed vs primary
const WAVE_FLOOR:   f32 = 0.35; // wave troughs still glow - a fire's base never goes black
const HEAT_BIAS:    f32 = 0.45; // warms the whole flame so the base idles white-hot
const COOL_TILT:    f32 = 0.35;  // world tilt: heat subtracted by the top row - offsets each
                                // row's range cooler without crushing its variance
const TILE_TILT:    f32 = 0.25; // per-tile tilt: subtracted at each triangle's own top edge,
                                // so every tile carries its own bottom-to-top fade
const FLICKER_DEPTH: f32 = 0.35; // bipolar: spikes above 1.0 push wavecrests into white/blue
const FLICKER_PERIOD_MS:  u32 = 628; // two incommensurate periods so the flicker
const FLICKER2_PERIOD_MS: u32 = 401; // reads as noise, not a synchronized wobble
const BREATHE_DEPTH: f32 = 0.15; // slow whole-flame swell
const BREATHE_PERIOD_MS: u32 = 3_700;
// Smoke: a drifting nested-sine field; where it crosses its threshold band, heat is
// occluded into dark wisps that rise through the flame and meander sideways.
const SMOKE_DARKEN:  f32 = 0.7;   // how black a wisp's core gets (0 = no smoke)
const SMOKE_BAND_MM: f32 = 250.0; // vertical spacing between wisps
const SMOKE_MEANDER: f32 = 2.0;   // sideways wiggle depth (radians of the field)
const SMOKE_MEANDER_MM: f32 = 300.0; // horizontal wavelength of the wiggle
const SMOKE_EDGE0: f32 = 0.72; // field value where a wisp starts fading in...
const SMOKE_EDGE1: f32 = 0.88; // ...and where its core is fully dark
const SMOKE_RISE_PERIOD_MS:    u32 = 5_100; // wisps climb one band spacing per period
const SMOKE_MEANDER_PERIOD_MS: u32 = 8_900;
// Occasional events - hash-scheduled and deterministic, so previews are reproducible.
const EMBER_COUNT: usize = 5;        // ember flights aloft at once
const EMBER_PERIOD_MS:  u32 = 2_600; // base flight time; each slot staggers longer
const EMBER_STAGGER_MS: u32 = 977;   // per-slot period offset (keeps respawns unsynced)
const EMBER_RISE_MM:   f32 = 260.0;  // how far an ember climbs before fading out
const EMBER_WOBBLE_MM: f32 = 25.0;   // sideways drift while rising
const EMBER_RADIUS_MM: f32 = 28.0;   // glow blob radius
const EMBER_HEAT:      f32 = 0.55;   // heat added at the blob center
const FLARE_PERIOD_MS: u32 = 8_300;  // one hash-picked tile flares up per period
const FLARE_LEN:  f32 = 0.25;        // flare duration as a fraction of its period
const FLARE_HEAT: f32 = 0.3;         // heat added across the flaring tile

pub struct ApexFlame {
    pub speed:      f32, // mm/s outward, primary wave
    pub wavelength: f32, // mm per cycle, primary wave
    // Each wave's per-LED phase as a phasor. Both wave arguments are a fixed per-LED term plus
    // a per-frame one, so rotating these by the frame's angle replaces a sin call per LED.
    w1_sin: [f32; LED_COUNT],
    w1_cos: [f32; LED_COUNT],
    w2_sin: [f32; LED_COUNT],
    w2_cos: [f32; LED_COUNT],
}

impl ApexFlame {
    pub fn new(speed: f32, wavelength: f32) -> Self {
        let k1 = TAU / wavelength;
        let k2 = TAU / (wavelength * SECOND_WAVELENGTH_RATIO);
        ApexFlame {
            speed,
            wavelength,
            w1_sin: core::array::from_fn(|i| (DIST_APEX[i] * k1).sin()),
            w1_cos: core::array::from_fn(|i| (DIST_APEX[i] * k1).cos()),
            w2_sin: core::array::from_fn(|i| (DIST_APEX[i] * k2).sin()),
            w2_cos: core::array::from_fn(|i| (DIST_APEX[i] * k2).cos()),
        }
    }
}

impl Pattern for ApexFlame {
    fn render(&mut self, leds: &[Led], t_ms: u32, out: &mut Frame) {
        // Fold each time term to its own period before the f32 cast (long-uptime precision).
        let wl2  = self.wavelength * SECOND_WAVELENGTH_RATIO;
        let spd2 = self.speed * SECOND_SPEED_RATIO;
        let p1_ms = (self.wavelength / self.speed * 1000.0) as u32;
        let p2_ms = (wl2 / spd2 * 1000.0) as u32;
        let t1_s = (t_ms % p1_ms.max(1)) as f32 / 1000.0;
        let t2_s = (t_ms % p2_ms.max(1)) as f32 / 1000.0;
        let flick_phase   = (t_ms % FLICKER_PERIOD_MS) as f32 / FLICKER_PERIOD_MS as f32 * TAU;
        let flick2_phase  = (t_ms % FLICKER2_PERIOD_MS) as f32 / FLICKER2_PERIOD_MS as f32 * TAU;
        let breathe_phase = (t_ms % BREATHE_PERIOD_MS) as f32 / BREATHE_PERIOD_MS as f32 * TAU;
        let breathe = 1.0 - BREATHE_DEPTH * (0.5 + 0.5 * breathe_phase.sin());
        let smoke_rise    = (t_ms % SMOKE_RISE_PERIOD_MS) as f32 / SMOKE_RISE_PERIOD_MS as f32 * TAU;
        let smoke_meander = (t_ms % SMOKE_MEANDER_PERIOD_MS) as f32 / SMOKE_MEANDER_PERIOD_MS as f32 * TAU;
        let extents = board_y_extents(leds);

        // Per-frame rotation angles for the two waves and the two flicker sines. Each pairs
        // with a per-LED phasor below, replacing four sin calls per LED with multiply-adds.
        let (b1_sin, b1_cos) = (-t1_s * self.speed * (TAU / self.wavelength)).sin_cos();
        let (b2_sin, b2_cos) = (-t2_s * spd2 * (TAU / wl2)).sin_cos();
        let (f1_sin, f1_cos) = flick_phase.sin_cos();
        let (f2_sin, f2_cos) = flick2_phase.sin_cos();
        let (fl_sin, fl_cos) = phase_phasors();

        // Ember flights: each slot is a scheduled, deterministic arc - spawn inside the
        // triangle's width low down, rise with a sideways wobble, fade out (sin envelope).
        let mut embers = [(0.0f32, 0.0f32, 0.0f32); EMBER_COUNT]; // (x, y, strength)
        for (k, ember) in embers.iter_mut().enumerate() {
            let period = EMBER_PERIOD_MS + k as u32 * EMBER_STAGGER_MS;
            let cycle = t_ms / period;
            let phase = (t_ms % period) as f32 / period as f32;
            let h = cycle.wrapping_mul(2654435761) ^ (k as u32).wrapping_mul(0x9E37_79B9);
            let y0 = WORLD_BOT - 6.0 - ((h >> 8) % 130) as f32;
            let half_w = (WORLD_BOT - y0) / WORLD_H * 250.0; // triangle half-width at y0
            let x0 = WORLD_CX + ((h % 201) as f32 - 100.0) / 100.0 * half_w;
            let x = x0 + (phase * TAU * 1.5 + (h >> 16) as f32).sin() * EMBER_WOBBLE_MM;
            let y = y0 - phase * EMBER_RISE_MM;
            *ember = (x, y, (phase * PI).sin());
        }

        // Tile flare-up: one hash-picked tile per period surges fast and settles slowly.
        let flare_cycle = t_ms / FLARE_PERIOD_MS;
        let flare_phase = (t_ms % FLARE_PERIOD_MS) as f32 / FLARE_PERIOD_MS as f32;
        let flare_board = 1 + (flare_cycle.wrapping_mul(2654435761) >> 8) % 25;
        let flare_env = if flare_phase < FLARE_LEN {
            let fp = flare_phase / FLARE_LEN;
            (fp * 6.0).min(1.0) * (1.0 - fp)
        } else {
            0.0
        };

        for (i, led) in leds.iter().enumerate() {
            // Two interfering ripples so the wavefronts don't look mechanical.
            let w1 = self.w1_sin[i] * b1_cos + self.w1_cos[i] * b1_sin;
            let w2 = self.w2_sin[i] * b2_cos + self.w2_cos[i] * b2_sin;
            let wave = ((w1 + SECOND_WAVE_STRENGTH * w2) / (1.0 + SECOND_WAVE_STRENGTH) + 1.0) / 2.0;
            let wave = WAVE_FLOOR + (1.0 - WAVE_FLOOR) * wave;

            // Subtractive cooling, two layers: world height so each row of triangles is
            // its own temperature band, plus a per-tile gradient that resets on every
            // triangle so each tile fades from hot at its own bottom to cool at its top.
            let height = (WORLD_BOT - led.wy) / WORLD_H; // 0 at the bottom tip, 1 at the top row
            let (y_min, y_max) = extents[led.board_id as usize];
            let local_height = (y_max - led.wy) / (y_max - y_min).max(1.0);
            let cooling = height * COOL_TILT + local_height * TILE_TILT;

            // Smoke field: rising bands whose vertical position wiggles sideways; where
            // the field crosses its threshold band, heat is occluded into a dark wisp.
            let field = ((led.wy / SMOKE_BAND_MM * TAU + smoke_rise
                + SMOKE_MEANDER * (led.wx / SMOKE_MEANDER_MM * TAU + smoke_meander).sin())
                .sin() + 1.0) / 2.0;
            let wisp = ((field - SMOKE_EDGE0) / (SMOKE_EDGE1 - SMOKE_EDGE0)).clamp(0.0, 1.0);
            let wisp = wisp * wisp * (3.0 - 2.0 * wisp); // soft edges
            let smoke = 1.0 - SMOKE_DARKEN * wisp;

            // Two flicker sines per LED, bipolar around 1.0 so crests can overshoot into
            // the white/blue ramp top. Phases come from an integer hash of the chain index
            // (transition.rs's sparkle trick): a linear phase step along the chain would
            // read as a coherent sweep across the fixture instead of random flicker.
            let h = (led.chain_idx as u32).wrapping_mul(2654435761);
            let k1 = (h % PHASE_STEPS as u32) as usize;
            let k2 = ((h >> 16) % PHASE_STEPS as u32) as usize; // decorrelated second phase
            let flicker = 1.0 + FLICKER_DEPTH * 0.5
                * (fl_sin[k1] * f1_cos + fl_cos[k1] * f1_sin
                 + fl_sin[k2] * f2_cos + fl_cos[k2] * f2_sin);

            // Occasional events: any nearby ember blobs plus the flaring tile.
            let mut event_heat = 0.0f32;
            for &(ex, ey, strength) in &embers {
                let d2 = (led.wx - ex).powi(2) + (led.wy - ey).powi(2);
                event_heat += EMBER_HEAT * strength
                    * (1.0 - d2 / (EMBER_RADIUS_MM * EMBER_RADIUS_MM)).max(0.0);
            }
            if led.board_id as u32 == flare_board {
                event_heat += FLARE_HEAT * flare_env;
            }

            out[i] = fire_ramp((wave * flicker * breathe + HEAT_BIAS - cooling + event_heat) * smoke);
        }
    }
}

/// Per-board vertical extent (min wy, max wy) indexed by board_id (1..=25), computed
/// once - the geometry is fixed. Drives the per-tile gradient for both orientations.
fn board_y_extents(leds: &[Led]) -> &'static [(f32, f32); 26] {
    static EXTENTS: OnceLock<[(f32, f32); 26]> = OnceLock::new();
    EXTENTS.get_or_init(|| {
        let mut ext = [(f32::MAX, f32::MIN); 26];
        for led in leds {
            let b = led.board_id as usize;
            ext[b].0 = ext[b].0.min(led.wy);
            ext[b].1 = ext[b].1.max(led.wy);
        }
        ext
    })
}

/// Blackbody-ish heat ramp: coal-ember -> deep red -> orange -> bright yellow -> white ->
/// blue-white. The floor is a dim ember rather than pure black, so cooled tops and smoke
/// wisps glow as dark coals instead of switching fully off. With the height cooling this
/// puts blue-white at the flame's base and a red flameout at the top rows, like a real flame.
fn fire_ramp(heat: f32) -> [u8; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.00, [25.0, 3.0, 0.0]),      // dim coal-ember floor (never fully off)
        (0.30, [180.0, 10.0, 0.0]),    // deep red
        (0.55, [255.0, 110.0, 0.0]),   // orange
        (0.75, [255.0, 230.0, 40.0]),  // bright yellow
        (0.90, [255.0, 255.0, 200.0]), // white
        (1.00, [170.0, 210.0, 255.0]), // blue-white hottest core
    ];
    let h = heat.clamp(0.0, 1.0);
    for pair in STOPS.windows(2) {
        let (h0, c0) = pair[0];
        let (h1, c1) = pair[1];
        if h <= h1 {
            let t = (h - h0) / (h1 - h0);
            return [
                lerp(c0[0], c1[0], t) as u8,
                lerp(c0[1], c1[1], t) as u8,
                lerp(c0[2], c1[2], t) as u8,
            ];
        }
    }
    [170, 210, 255]
}
