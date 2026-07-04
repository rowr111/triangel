use std::sync::OnceLock;

use crate::patterns::{Frame, Pattern, lerp};
use crate::led::map::{Led, WORLD_BOT, WORLD_CX, WORLD_H};
use core::f32::consts::TAU;

// Tunables - dial these in the previewer. Each scales one ingredient of the flame;
// pushing one toward zero removes that ingredient.
const SECOND_WAVE_STRENGTH:    f32 = 0.5;  // interference wave amplitude vs the primary
const SECOND_WAVELENGTH_RATIO: f32 = 1.7;  // second wave's wavelength vs primary (incommensurate)
const SECOND_SPEED_RATIO:      f32 = 0.63; // second wave's speed vs primary
const WAVE_FLOOR:   f32 = 0.2;  // wave troughs still glow - a fire's base never goes black
const HEAT_BIAS:    f32 = 0.2;  // warms the whole flame so the base idles yellow, not orange
const COOL_TILT:    f32 = 0.5;  // world tilt: heat subtracted by the top row - offsets each
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

pub struct ApexFlame {
    pub speed:      f32, // mm/s outward, primary wave
    pub wavelength: f32, // mm per cycle, primary wave
}

impl Pattern for ApexFlame {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
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

        for (i, led) in leds.iter().enumerate() {
            // Flames rise: the apex of the point-down triangle is the fire's base.
            let dist = led.dist_to(WORLD_CX, WORLD_BOT);

            // Two interfering ripples so the wavefronts don't look mechanical.
            let w1 = ((dist - t1_s * self.speed) / self.wavelength * TAU).sin();
            let w2 = ((dist - t2_s * spd2) / wl2 * TAU).sin();
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

            // Two decorrelated flicker sines per LED (CenterShimmer's hash-phase trick),
            // bipolar around 1.0 so crests can overshoot into the white/blue ramp top.
            let h1 = (led.board_id as u32 * 7 + led.local_idx as u32 * 13) % 97;
            let h2 = (led.board_id as u32 * 31 + led.local_idx as u32 * 17) % 89;
            let flicker = 1.0 + FLICKER_DEPTH * 0.5
                * ((flick_phase + h1 as f32).sin() + (flick2_phase + h2 as f32).sin());

            out[i] = fire_ramp((wave * flicker * breathe + HEAT_BIAS - cooling) * smoke);
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

/// Blackbody-ish heat ramp: black -> deep red -> orange -> bright yellow -> white ->
/// blue-white. With the apex falloff this puts blue at the flame's base (bottom tip)
/// and a red flameout at the top corners, like a real flame.
fn fire_ramp(heat: f32) -> [u8; 3] {
    const STOPS: [(f32, [f32; 3]); 6] = [
        (0.00, [0.0, 0.0, 0.0]),
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
