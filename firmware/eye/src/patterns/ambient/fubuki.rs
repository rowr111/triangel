use crate::patterns::{Frame, Pattern, hsv, lerp, wrap360};
use crate::led::grid::{self, CELL_MM};
use crate::led::map::{Led, WORLD_TOP, WORLD_BOT, WORLD_H, WORLD_CX, LED_COUNT};
use core::f32::consts::TAU;

// Fubuki (snowstorm) - a falling-snow ambient pattern that turns with the seasons. Flakes
// drift down a dark sky and funnel toward the apex (the point-down triangle is a funnel);
// they settle, and a pile rises from the bottom point until the triangle is full; the old
// season then melts back down while the next rises through it by a small overlap (no empty
// gap): spring petals -> summer green -> autumn leaves -> winter snow -> back. The falling
// thing and the pile take the season's colors.
//
// Holds only a tiny re-entry clock (so it restarts from empty each time you enter the
// pattern); the season and fill level otherwise fall out of that clock. Flakes are a hashed
// emitter (like Effervesce). The per-LED hot loop is arithmetic + squared-distance flake dots
// (no trig), so it stays cheap on the FP-less chip.

// ============================== Tuning knobs ==============================

// Season cycle: one season lasts FILL_MS; the next cross-dissolves in as it fills.
const FILL_MS:        u32 = 45_000; // one season's cycle; 4 x 45s = the ~3-min setlist window, so a full year fits per showing
const FILL_FRAC:      f32 = 0.97;   // point in the cycle the new pile reaches full (the rest holds)
const FILL_DELAY:     f32 = 0.2;    // new pile waits this long before rising, so the old melts down in its own color first; the new color arrives on the falling flakes meanwhile
const RECEDE_FRAC:    f32 = 0.25;   // fraction of the cycle the old season takes to melt away
const COLOR_START:    f32 = 0.15; // new color starts turning on here (into the melt)
const COLOR_SPAN:     f32 = 0.12; // over this much of the cycle the flakes turn old -> new one by one

// Sky + settled snow.
const SKY_VAL:      f32 = 0.1;  // sky / empty brightness (a soft glow, not full black)
const PILE_VAL:     f32 = 0.9;   // settled-snow brightness
const PILE_TEXTURE: f32 = 0.2;   // per-LED brightness variation in the pile (0 = flat)
const PILE_PULSE:      f32 = 0.7;  // settled-snow random pulse depth (0 = static)
const PILE_PULSE_RATE: f32 = 0.003; // pulse speed (rad/ms)
const FILL_FEATHER_MM: f32 = 22.0; // soft vertical depth of the fill front
const FILL_JITTER_MM:  f32 = 38.0; // per-LED random offset so rows fill raggedly, not all at once
const FILL_EXP:        f32 = 0.78; // fill curve: 0.5 = even area (slow top), 1.0 = even height (fast top)

// Flakes.
const FLAKES:      usize = 16;   // flakes falling at once
const FLAKE_R_MM:  f32   = 22.0; // soft dot radius
const FLAKE_VAL:   f32   = 1.0;  // flake brightness (peaks at full)
const FALL_MIN_MS: u32   = 4_500;// fastest fall (each flake picks its own -> mixed speeds)
const FALL_MAX_MS: u32   = 9_000;// slowest fall
const SWAY_MM:     f32   = 30.0; // horizontal sway amplitude
const SWAY_CYCLES: f32   = 1.5;  // sway oscillations over one fall
const CONVERGE:    f32   = 0.5;  // how far flakes drift toward center as they fall (funnel)

// A season's colors: a dark sky tint, plus a small palette (hue, sat) shared by the pile
// and the flakes. `n` is how many of `colors` are used.
struct Season {
    sky_hue: f32,
    sky_sat: f32,
    n:       u32,
    colors:  [(f32, f32); 12],
}

const PAD: (f32, f32) = (0.0, 0.0); // unused palette slots

const SEASONS: [Season; 4] = [
    // Spring: mostly sakura pink (~67%), a little white (~25%), a small touch of fresh green (~8%).
    Season { sky_hue: 330.0, sky_sat: 0.35, n: 12,
        colors: [(332.0, 0.6), (0.0, 0.0), (345.0, 0.7), (332.0, 0.6), (345.0, 0.7), (0.0, 0.0),
                 (332.0, 0.6), (345.0, 0.7), (100.0, 0.6), (332.0, 0.6), (345.0, 0.7), (0.0, 0.0)] },
    // Summer: mostly bright green (three shades), ~1/8 warm tan, ~1/8 golden yellow.
    Season { sky_hue: 120.0, sky_sat: 0.4, n: 8,
        colors: [(105.0, 0.9), (90.0, 0.85), (130.0, 0.8), (105.0, 0.9), (90.0, 0.85), (130.0, 0.8),
                 (30.0, 0.5), (44.0, 0.92), PAD, PAD, PAD, PAD] },
    // Autumn: gold / orange / rust / red leaves, warm sky.
    Season { sky_hue: 25.0,  sky_sat: 0.5,  n: 4,
        colors: [(40.0, 0.9), (25.0, 0.95), (12.0, 0.95), (0.0, 0.85), PAD, PAD, PAD, PAD, PAD, PAD, PAD, PAD] },
    // Winter: white snow with a hint of pale blue, cold night sky.
    Season { sky_hue: 215.0, sky_sat: 0.55, n: 2,
        colors: [(0.0, 0.0), (210.0, 0.22), PAD, PAD, PAD, PAD, PAD, PAD, PAD, PAD, PAD, PAD] },
];

// One falling flake, positioned for the current frame.
#[derive(Clone, Copy)]
struct Flake {
    x:   f32,
    y:   f32,
    hue: f32,
    sat: f32,
}

pub struct Fubuki {
    origin_ms: u32, // local-clock origin: t_ms when the current activation began
    prev_ms:   u32, // t_ms of the previous render call, to detect a re-entry gap
    active:    bool,
    // Strongest flake reaching each LED this frame, and the color it carries. Rebuilt per
    // frame by scattering the flakes onto nearby LEDs.
    flake_e: [f32; LED_COUNT],
    flake_h: [f32; LED_COUNT],
    flake_s: [f32; LED_COUNT],
}

impl Fubuki {
    pub fn new() -> Self {
        Fubuki {
            origin_ms: 0,
            prev_ms:   0,
            active:    false,
            flake_e:   [0.0; LED_COUNT],
            flake_h:   [0.0; LED_COUNT],
            flake_s:   [0.0; LED_COUNT],
        }
    }
}

impl Pattern for Fubuki {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        // Restart the season cycle whenever we (re)enter the pattern - at boot, or when it's
        // switched to. The pattern isn't rendered while off-screen, so a gap since the last
        // render means we just came back; reset the local clock to start from an empty triangle.
        const REENTRY_GAP_MS: u32 = 500;
        if !self.active || t_ms.wrapping_sub(self.prev_ms) > REENTRY_GAP_MS {
            self.origin_ms = t_ms;
            self.active = true;
        }
        self.prev_ms = t_ms;
        let local = t_ms.wrapping_sub(self.origin_ms);

        // Current season (rising) and the previous one (melting away) run at once near the
        // boundary, so the old fades out while a small amount of the new is already rising
        // through - an overlap, not a fade to black.
        let gen = local / FILL_MS;
        let ct  = local % FILL_MS; // ms into the current season
        let season = &SEASONS[(gen % SEASONS.len() as u32) as usize];
        let prev   = &SEASONS[((gen + SEASONS.len() as u32 - 1) % SEASONS.len() as u32) as usize];

        // Old season melts back down over the first RECEDE_FRAC (its fade-out, in its own
        // color). The new pile waits until FILL_DELAY before rising, so it doesn't recolor the
        // melting old one - the new color arrives on the falling flakes until then. On the very
        // first cycle after (re)entry there is no previous season: start from an empty triangle
        // and just fill with the first palette (no melt, no fill delay).
        let first = gen == 0;
        let delay = if first { 0.0 } else { FILL_DELAY };
        let fc = (((ct as f32 / FILL_MS as f32) - delay) / (FILL_FRAC - delay)).clamp(0.0, 1.0);
        let fp = if first { 0.0 } else { 1.0 - (ct as f32 / (FILL_MS as f32 * RECEDE_FRAC)).min(1.0) };
        let line_c = fill_line(fc);
        let line_p = fill_line(fp);
        let jscale_c = 4.0 * fc * (1.0 - fc);
        let jscale_p = 4.0 * fp * (1.0 - fp);

        // Folded time for the settled-snow pulse.
        let pulse_t = (local % ((TAU / PILE_PULSE_RATE) as u32).max(1)) as f32;

        // Color turn: over COLOR_SPAN starting at COLOR_START, the new season takes over as a
        // rising fraction `nf` - the sky tint crossfades, and each falling flake individually
        // flips from the old color to the new, so the snow shifts flake-by-flake rather than all
        // at once. On the first cycle there's no old season, so it's all new immediately.
        let nf = if first {
            1.0
        } else {
            ((ct as f32 / FILL_MS as f32 - COLOR_START) / COLOR_SPAN).clamp(0.0, 1.0)
        };
        let sky_h = blend_hue(prev.sky_hue, season.sky_hue, nf);
        let sky_s = lerp(prev.sky_sat, season.sky_sat, nf);

        // Precompute the falling flakes; they land on whichever pile is currently highest (the
        // old one while it's still melting down, then the new one as it builds up).
        let surface = line_c.min(line_p);
        let flakes: [Flake; FLAKES] = core::array::from_fn(|i| make_flake(i, local, surface, prev, season, nf));

        // Scatter the flakes onto the LEDs they actually reach, keeping the strongest per LED.
        // Flakes are walked in order and the test is strict, so ties land on the same flake a
        // per-LED sweep would have picked.
        let reach = (FLAKE_R_MM / CELL_MM).ceil() as usize;
        let (fe_buf, fh_buf, fs_buf) = (&mut self.flake_e, &mut self.flake_h, &mut self.flake_s);
        fe_buf.fill(0.0);
        for f in &flakes {
            grid::for_each_near(f.x, f.y, reach, |k| {
                let led = &leds[k];
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

        for (i, led) in leds.iter().enumerate() {
            // Per-LED texture + pulse (season-independent), shared by both piles.
            let hp = hash2(led.board_id as u32, led.local_idx as u32);
            let tex = 1.0 - PILE_TEXTURE * ((hp >> 9 & 0xFF) as f32 / 255.0);
            let phase = (hp >> 17 & 0x1FF) as f32 / 512.0 * TAU;
            let pulse = 1.0 - PILE_PULSE * (0.5 - 0.5 * (pulse_t * PILE_PULSE_RATE + phase).sin());
            let pile_v = PILE_VAL * tex * pulse;

            // Ragged per-LED fill fronts for both piles (jitter tapers to 0 at empty/full).
            let jbase = (hp >> 26) as f32 / 63.0 - 0.5;
            let fillamt_c = ((led.wy - line_c - jbase * 2.0 * FILL_JITTER_MM * jscale_c) / FILL_FEATHER_MM).clamp(0.0, 1.0);
            let fillamt_p = ((led.wy - line_p - jbase * 2.0 * FILL_JITTER_MM * jscale_p) / FILL_FEATHER_MM).clamp(0.0, 1.0);

            // Composite: season-tinted dark sky, then the old pile melting away over it, then
            // the new pile rising on top - so the new overlaps the old as it fades out.
            let (pch, pcs) = prev.colors[(hp % prev.n) as usize];
            let (ch, cs) = season.colors[(hp % season.n) as usize];
            let mut hue = sky_h;
            let mut sat = sky_s;
            let mut val = SKY_VAL;
            hue = blend_hue(hue, pch, fillamt_p);
            sat = lerp(sat, pcs, fillamt_p);
            val = lerp(val, pile_v, fillamt_p);
            hue = blend_hue(hue, ch, fillamt_c);
            sat = lerp(sat, cs, fillamt_c);
            val = lerp(val, pile_v, fillamt_c);

            // Blend the strongest flake's color over the base and brighten it in. At zero
            // strength the blends are identities, so LEDs no flake reached are left alone.
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

/// Position and color a flake for the current frame (stateless: hashed per slot + generation).
fn make_flake(i: usize, t_ms: u32, fill_y: f32, prev: &Season, new: &Season, nf: f32) -> Flake {
    // Per-slot random fall speed, with a random phase so they don't drop in sync.
    let sseed  = hash2(i as u32 + 1, 0x0F1A);
    let period = FALL_MIN_MS + sseed % (FALL_MAX_MS - FALL_MIN_MS);
    let local  = t_ms.wrapping_add(sseed % period);
    let gen    = local / period;
    let fp     = (local % period) as f32 / period as f32; // 0 at top -> 1 at the pile surface

    // Fresh spawn each generation: x across the top, a sway phase, a season color.
    let g       = hash2(i as u32 + 1, gen);
    let spawn_x = 20.0 + (g % 481) as f32;
    let sway_ph = (g >> 8 & 0xFF) as f32 / 255.0 * TAU;

    // Drift toward center as it falls (funnel), plus a gentle horizontal sway.
    let x = lerp(spawn_x, WORLD_CX, fp * CONVERGE) + SWAY_MM * (fp * SWAY_CYCLES * TAU + sway_ph).sin();
    let y = WORLD_TOP + fp * (fill_y - WORLD_TOP);
    // Per-flake old/new color: a fraction `nf` of flakes have already turned to the new season.
    let src = if (((g >> 20) & 0x3FF) as f32 / 1024.0) < nf { new } else { prev };
    let (hue, sat) = src.colors[(g % src.n) as usize];

    Flake { x, y, hue, sat }
}

/// Height of the fill line for a fill fraction (0 empty at the apex -> 1 full at the top).
/// Fills by (roughly) area via a gentle power curve, with a small feather overshoot at each end.
fn fill_line(fr: f32) -> f32 {
    let margin = FILL_FEATHER_MM;
    (WORLD_BOT + margin) - (WORLD_H + 2.0 * margin) * fr.powf(FILL_EXP)
}

/// Blend hue `base` -> `target` (degrees) by `t`, along the shorter arc. Arithmetic only.
fn blend_hue(base: f32, target: f32, t: f32) -> f32 {
    let mut diff = target - base;
    if diff > 180.0 {
        diff -= 360.0;
    } else if diff < -180.0 {
        diff += 360.0;
    }
    base + diff * t
}

/// Bit-mix hash of two u32s into a scrambled u32.
fn hash2(a: u32, b: u32) -> u32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0x27D4_EB2F);
    h ^= h >> 13;
    h
}
