use crate::audio::Audio;
use crate::patterns::{Frame, Pattern, cycle, fold_ms, hash2, hsv, lerp, sin_sum, wrap360};
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{WORLD_TOP, WORLD_BOT, WORLD_H, WORLD_CX, LED_COUNT, LED_MAP};
use core::f32::consts::TAU;

// Fubuki (snowstorm): flakes fall and pile up from the apex until the triangle is full,
// then the pile melts down while the next season's rises: spring, summer, autumn, winter.
// Flakes and pile take the season's colors.

// Season cycle, as fractions of FILL_MS
const FILL_MS:        u32 = 45_000; // one season; four fit the 3-minute setlist slot
const FILL_FRAC:      f32 = 0.97;   // when the new pile is full
const FILL_DELAY:     f32 = 0.2;    // when the new pile starts rising
const RECEDE_FRAC:    f32 = 0.25;   // how long the old pile takes to melt
const COLOR_START:    f32 = 0.15; // when flakes start turning to the new colors
const COLOR_SPAN:     f32 = 0.12; // how long that takes, flake by flake

// Sky and pile
const SKY_VAL:      f32 = 0.1;  // sky brightness
const PILE_VAL:     f32 = 0.9;   // pile brightness
const PILE_TEXTURE: f32 = 0.2;   // per-LED brightness variation in the pile
const PILE_PULSE:      f32 = 0.7;  // depth of the pile's per-LED pulse
const PILE_PULSE_RATE: f32 = 0.003; // pulse speed (rad/ms)
const FILL_FEATHER_MM: f32 = 22.0; // softness of the pile's top edge
const FILL_JITTER_MM:  f32 = 38.0; // per-LED offset that makes the edge ragged
const FILL_EXP:        f32 = 0.78; // fill curve: 0.5 fills by even area, 1.0 by even height

// Flakes
const FLAKES:      usize = 16;   // flakes falling at once
const FLAKE_R_MM:  f32   = 22.0; // dot radius
const FLAKE_VAL:   f32   = 1.0;  // flake brightness
const FALL_MIN_MS: u32   = 4_500;// each flake keeps its own fall time in this range
const FALL_MAX_MS: u32   = 9_000;
const SWAY_MM:     f32   = 30.0; // sideways sway
const SWAY_CYCLES: f32   = 1.5;  // sways per fall
const CONVERGE:    f32   = 0.5;  // how far flakes drift toward the center as they fall

// A sky tint plus the (hue, sat) palette shared by pile and flakes.
struct Season {
    sky_hue: f32,
    sky_sat: f32,
    colors:  &'static [(f32, f32)],
}

impl Season {
    /// The palette color a hash picks.
    fn color(&self, hash: u32) -> (f32, f32) {
        self.colors[hash as usize % self.colors.len()]
    }
}

const SEASONS: [Season; 4] = [
    // Spring: mostly pink, some white, a touch of green.
    Season { sky_hue: 330.0, sky_sat: 0.35,
        colors: &[(332.0, 0.6), (0.0, 0.0), (345.0, 0.7), (332.0, 0.6), (345.0, 0.7), (0.0, 0.0),
                  (332.0, 0.6), (345.0, 0.7), (100.0, 0.6), (332.0, 0.6), (345.0, 0.7), (0.0, 0.0)] },
    // Summer: three greens, a tan and a golden yellow.
    Season { sky_hue: 120.0, sky_sat: 0.4,
        colors: &[(105.0, 0.9), (90.0, 0.85), (130.0, 0.8), (105.0, 0.9), (90.0, 0.85), (130.0, 0.8),
                  (30.0, 0.5), (44.0, 0.92)] },
    // Autumn: gold, orange, rust and red.
    Season { sky_hue: 25.0,  sky_sat: 0.5,
        colors: &[(40.0, 0.9), (25.0, 0.95), (12.0, 0.95), (0.0, 0.85)] },
    // Winter: white and pale blue.
    Season { sky_hue: 215.0, sky_sat: 0.55,
        colors: &[(0.0, 0.0), (210.0, 0.22)] },
];

#[derive(Clone, Copy)]
struct Flake {
    x:   f32,
    y:   f32,
    hue: f32,
    sat: f32,
}

pub struct Fubuki {
    origin_ms: u32, // t_ms when the pattern last came on screen
    // Strongest flake on each LED this frame, and its hue and saturation.
    flake_e: [f32; LED_COUNT],
    flake_h: [f32; LED_COUNT],
    flake_s: [f32; LED_COUNT],
    // Fixed per LED, from a hash of its board and position: the hash, the pile brightness
    // with its texture, sin and cos of the pulse phase, and the ragged-edge offset (mm).
    led_hash:  [u32; LED_COUNT],
    pile_tex:  [f32; LED_COUNT],
    pulse_sin: [f32; LED_COUNT],
    pulse_cos: [f32; LED_COUNT],
    jitter_mm: [f32; LED_COUNT],
}

impl Fubuki {
    pub fn new() -> Self {
        let led_hash: [u32; LED_COUNT] =
            core::array::from_fn(|i| hash2(LED_MAP[i].board_id as u32, LED_MAP[i].local_idx as u32));
        let pulse_at = |hp: u32| (hp >> 17 & 0x1FF) as f32 / 512.0 * TAU;
        Fubuki {
            origin_ms: 0,
            flake_e:   [0.0; LED_COUNT],
            flake_h:   [0.0; LED_COUNT],
            flake_s:   [0.0; LED_COUNT],
            led_hash,
            pile_tex:  led_hash.map(|hp| PILE_VAL * (1.0 - PILE_TEXTURE * ((hp >> 9 & 0xFF) as f32 / 255.0))),
            pulse_sin: led_hash.map(|hp| pulse_at(hp).sin()),
            pulse_cos: led_hash.map(|hp| pulse_at(hp).cos()),
            jitter_mm: led_hash.map(|hp| ((hp >> 26) as f32 / 63.0 - 0.5) * 2.0 * FILL_JITTER_MM),
        }
    }
}

impl Pattern for Fubuki {
    fn on_enter(&mut self, t_ms: u32) {
        // Start over from an empty triangle.
        self.origin_ms = t_ms;
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let local = t_ms.wrapping_sub(self.origin_ms);

        let gen = local / FILL_MS;
        let ct  = local % FILL_MS; // ms into the current season
        let season = &SEASONS[(gen % SEASONS.len() as u32) as usize];
        let prev   = &SEASONS[((gen + SEASONS.len() as u32 - 1) % SEASONS.len() as u32) as usize];

        // fc and fp are how full the current and previous seasons' piles are. The first
        // season after entry has no previous pile to melt and starts filling at once.
        let first = gen == 0;
        let delay = if first { 0.0 } else { FILL_DELAY };
        let fc = (((ct as f32 / FILL_MS as f32) - delay) / (FILL_FRAC - delay)).clamp(0.0, 1.0);
        let fp = if first { 0.0 } else { 1.0 - (ct as f32 / (FILL_MS as f32 * RECEDE_FRAC)).min(1.0) };
        let line_c = fill_line(fc);
        let line_p = fill_line(fp);
        let jscale_c = 4.0 * fc * (1.0 - fc);
        let jscale_p = 4.0 * fp * (1.0 - fp);

        let (ph_sin, ph_cos) = (fold_ms(local, PILE_PULSE_RATE) * PILE_PULSE_RATE).sin_cos();

        // nf is how far the sky and the flakes have turned to the new season's colors.
        let nf = if first {
            1.0
        } else {
            ((ct as f32 / FILL_MS as f32 - COLOR_START) / COLOR_SPAN).clamp(0.0, 1.0)
        };
        let sky_h = blend_hue(prev.sky_hue, season.sky_hue, nf);
        let sky_s = lerp(prev.sky_sat, season.sky_sat, nf);

        // Flakes land on whichever pile is higher.
        let surface = line_c.min(line_p);
        let flakes: [Flake; FLAKES] = core::array::from_fn(|i| make_flake(i, local, surface, prev, season, nf));

        // Keep the strongest flake per LED.
        let reach = (FLAKE_R_MM / CELL_MM).ceil() as usize;
        let (fe_buf, fh_buf, fs_buf) = (&mut self.flake_e, &mut self.flake_h, &mut self.flake_s);
        fe_buf.fill(0.0);
        for f in &flakes {
            grid::for_each_near(f.x, f.y, reach, |k| {
                let led = &LED_MAP[k];
                let dx = led.wx - f.x;
                let dy = led.wy - f.y;
                let c = 1.0 - (dx * dx + dy * dy) / (FLAKE_R_MM * FLAKE_R_MM);
                if c > fe_buf[k] {
                    fe_buf[k] = c;
                    fh_buf[k] = f.hue;
                    fs_buf[k] = f.sat;
                }
            });
        }

        for (i, led) in LED_MAP.iter().enumerate() {
            // Per-LED texture and pulse, shared by both piles.
            let hp = self.led_hash[i];
            // sin(pulse phase + this LED's phase)
            let wave = sin_sum(ph_sin, ph_cos, self.pulse_sin[i], self.pulse_cos[i]);
            let pulse = 1.0 - PILE_PULSE * (0.5 - 0.5 * wave);
            let pile_v = self.pile_tex[i] * pulse;

            // How far inside each pile this LED is; the jitter shrinks to 0 at empty and full.
            let jitter = self.jitter_mm[i];
            let fillamt_c = ((led.wy - line_c - jitter * jscale_c) / FILL_FEATHER_MM).clamp(0.0, 1.0);
            let fillamt_p = ((led.wy - line_p - jitter * jscale_p) / FILL_FEATHER_MM).clamp(0.0, 1.0);

            // Sky, then the old pile over it, then the new pile on top.
            let (pch, pcs) = prev.color(hp);
            let (ch, cs) = season.color(hp);
            let mut hue = sky_h;
            let mut sat = sky_s;
            let mut val = SKY_VAL;
            hue = blend_hue(hue, pch, fillamt_p);
            sat = lerp(sat, pcs, fillamt_p);
            val = lerp(val, pile_v, fillamt_p);
            hue = blend_hue(hue, ch, fillamt_c);
            sat = lerp(sat, cs, fillamt_c);
            val = lerp(val, pile_v, fillamt_c);

            let fe = self.flake_e[i];
            if fe > 0.0 {
                hue = blend_hue(hue, self.flake_h[i], fe);
                sat = lerp(sat, self.flake_s[i], fe);
                val += (1.0 - val) * (fe * FLAKE_VAL);
            }

            out[i] = hsv(wrap360(hue), sat, val);
        }
    }
}

/// Flake `i`'s position and color this frame, from time and hashes.
fn make_flake(i: usize, t_ms: u32, fill_y: f32, prev: &Season, new: &Season, nf: f32) -> Flake {
    // Each slot has its own fall time and start offset, so the flakes do not drop together.
    let sseed  = hash2(i as u32 + 1, 0x0F1A);
    let period = FALL_MIN_MS + sseed % (FALL_MAX_MS - FALL_MIN_MS);
    let local  = t_ms.wrapping_add(sseed % period);
    let gen    = local / period;
    let fp     = cycle(local, period); // 0 at the top, 1 at the pile

    let g       = hash2(i as u32 + 1, gen);
    let spawn_x = 20.0 + (g % 481) as f32;
    let sway_ph = (g >> 8 & 0xFF) as f32 / 255.0 * TAU;

    let x = lerp(spawn_x, WORLD_CX, fp * CONVERGE) + SWAY_MM * (fp * SWAY_CYCLES * TAU + sway_ph).sin();
    let y = WORLD_TOP + fp * (fill_y - WORLD_TOP);
    // A fraction `nf` of the flakes use the new season's colors.
    let src = if (((g >> 20) & 0x3FF) as f32 / 1024.0) < nf { new } else { prev };
    let (hue, sat) = src.color(g);

    Flake { x, y, hue, sat }
}

/// World y of the pile's top edge for a fill fraction (0 empty, 1 full).
fn fill_line(fr: f32) -> f32 {
    let margin = FILL_FEATHER_MM;
    (WORLD_BOT + margin) - (WORLD_H + 2.0 * margin) * fr.powf(FILL_EXP)
}

/// Blends hue `base` toward `target` (degrees) by `t`, the short way around the wheel.
fn blend_hue(base: f32, target: f32, t: f32) -> f32 {
    let mut diff = target - base;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    base + diff * t
}
