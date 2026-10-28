use crate::audio::Audio;
use crate::led::grid;
use crate::led::map::{LED_COUNT, LED_MAP, WORLD_BOT, WORLD_CX, WORLD_H, WORLD_TOP};
use crate::led::world::{WORLD_LEFT, WORLD_RIGHT};
use crate::patterns::{
    hash01, hash2, hash_plus_minus, hsv, lerp, random_u32, rate_angle, slot_life, wrap360, Frame,
    Pattern, Phasor,
};
use crate::setlist::AMBIENT_CYCLE_MS;
use core::f32::consts::TAU;

// Season cycle, as fractions of SEASON_MS
const SEASON_MS: u32 = AMBIENT_CYCLE_MS / SEASONS.len() as u32; // one season
const FILL_START: f32 = 0.2; // when the new pile starts rising
const FILL_END: f32 = 0.97; // when the new pile is full
const MELT_SPAN: f32 = 0.25; // how long the old pile takes to melt
const COLOR_START: f32 = 0.15; // when flakes start turning to the new colors
const COLOR_SPAN: f32 = 0.12; // how long that takes, flake by flake

// Sky and pile
const SKY_VAL: f32 = 0.1; // sky brightness
const PILE_VAL: f32 = 0.9; // pile brightness
const PILE_TEXTURE: f32 = 0.2; // per-LED brightness variation in the pile
const PILE_PULSE: f32 = 0.7; // depth of the pile's per-LED pulse
const PILE_PULSE_RATE: f32 = 0.003; // pulse speed (rad/ms)
const FILL_FEATHER_MM: f32 = 22.0; // softness of the pile's top edge
const FILL_JITTER_MM: f32 = 38.0; // per-LED offset that makes the edge ragged
const FILL_EXP: f32 = 0.78; // fill curve: 0.5 fills by even area, 1.0 by even height

// Flakes
const FLAKES: usize = 16; // flakes falling at once
const FLAKE_R_MM: f32 = 22.0; // dot radius
const FLAKE_VAL: f32 = 1.0; // flake brightness
const FALL_MIN_MS: u32 = 4_500; // each flake keeps its own fall time in this range
const FALL_MAX_MS: u32 = 9_000;
// How far in from each side the flakes start.
const START_INSET_LEFT_MM: f32 = 10.0;
const START_INSET_RIGHT_MM: f32 = 5.0;
const SWAY_MM: f32 = 30.0; // sideways sway
const SWAY_CYCLES: f32 = 1.5; // sways per fall
const CONVERGE: f32 = 0.5; // how far flakes drift toward the center as they fall
const FLAKE_SALT: u32 = 0x0F1A; // mixed into each flake slot's hash

/// A sky tint plus the (hue, sat) palette shared by pile and flakes.
struct Season {
    sky_hue: f32,
    sky_sat: f32,
    colors: &'static [(f32, f32)],
}

impl Season {
    /// The palette color a hash picks.
    fn color(&self, hash: u32) -> (f32, f32) {
        self.colors[hash as usize % self.colors.len()]
    }
}

#[rustfmt::skip]
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

struct Flake {
    x: f32,
    y: f32,
    hue: f32,
    sat: f32,
}

/// A flake's strength and color on one LED.
#[derive(Clone, Copy)]
struct FlakeHit {
    strength: f32,
    hue: f32,
    sat: f32,
}

/// One season's pile.
struct Pile {
    top_y: f32, // world y of the top edge
    jitter_scale: f32,
}

impl Pile {
    /// A pile `fill` full, from 0 empty to 1 full.
    fn new(fill: f32) -> Self {
        Pile {
            top_y: pile_top_y(fill),
            jitter_scale: 4.0 * fill * (1.0 - fill),
        }
    }

    /// How far inside the pile an LED at `led_y` is, 0-1.
    fn depth(&self, led_y: f32, jitter: f32) -> f32 {
        ((led_y - self.top_y - jitter * self.jitter_scale) / FILL_FEATHER_MM).clamp(0.0, 1.0)
    }
}

/// Fubuki (snowstorm): flakes fall and pile up from the apex until the triangle is full,
/// then the pile melts down while the next season's rises: spring, summer, autumn, winter.
/// Flakes and pile take the season's colors.
pub struct Fubuki {
    origin_ms: u32, // t_ms when the pattern last came on screen
    seed: u32,      // new each time it comes on screen, so the snowfall differs
    // Strongest flake on each LED this frame, and its hue and saturation.
    flake_hits: [FlakeHit; LED_COUNT],
    // Fixed per LED, from a hash of its board and position: the hash, the pile brightness
    // with its texture, its offset in the pulse, and the ragged-edge offset (mm).
    led_hash: [u32; LED_COUNT],
    pile_brightness: [f32; LED_COUNT],
    pulse_offset: [Phasor; LED_COUNT],
    jitter_mm: [f32; LED_COUNT],
}

impl Fubuki {
    pub fn new() -> Self {
        let led_hash: [u32; LED_COUNT] = core::array::from_fn(|i| {
            hash2(LED_MAP[i].board_id as u32, LED_MAP[i].local_idx as u32)
        });
        Fubuki {
            origin_ms: 0,
            seed: 0,
            flake_hits: [FlakeHit {
                strength: 0.0,
                hue: 0.0,
                sat: 0.0,
            }; LED_COUNT],
            led_hash,
            pile_brightness: led_hash.map(|hash| PILE_VAL * (1.0 - PILE_TEXTURE * hash01(hash, 1))),
            pulse_offset: led_hash.map(|hash| Phasor::new(hash01(hash, 2) * TAU)),
            jitter_mm: led_hash.map(|hash| hash_plus_minus(hash, 3, FILL_JITTER_MM)),
        }
    }
}

impl Pattern for Fubuki {
    fn on_enter(&mut self, t_ms: u32) {
        // Start over from an empty triangle.
        self.origin_ms = t_ms;
        self.seed = random_u32();
    }

    fn render(&mut self, t_ms: u32, _audio: &Audio, out: &mut Frame) {
        let elapsed_ms = t_ms.wrapping_sub(self.origin_ms);

        let season_num = elapsed_ms / SEASON_MS;
        let into_season = elapsed_ms % SEASON_MS; // ms into the current season
        let new_season = &SEASONS[(season_num % SEASONS.len() as u32) as usize];
        let old_season =
            &SEASONS[((season_num + SEASONS.len() as u32 - 1) % SEASONS.len() as u32) as usize];

        // The first season after entry has no old pile to melt, so it starts filling at once.
        let first = season_num == 0;
        let delay = if first { 0.0 } else { FILL_START };
        let fill_new = (((into_season as f32 / SEASON_MS as f32) - delay) / (FILL_END - delay))
            .clamp(0.0, 1.0);
        let fill_old = if first {
            0.0
        } else {
            1.0 - (into_season as f32 / (SEASON_MS as f32 * MELT_SPAN)).min(1.0)
        };
        let new_pile = Pile::new(fill_new);
        let old_pile = Pile::new(fill_old);

        let pulse_phase = Phasor::new(rate_angle(elapsed_ms, PILE_PULSE_RATE));

        // new_colors is how far the sky and the flakes have turned to the new season's colors.
        let new_colors = if first {
            1.0
        } else {
            ((into_season as f32 / SEASON_MS as f32 - COLOR_START) / COLOR_SPAN).clamp(0.0, 1.0)
        };
        let sky_hue = blend_hue(old_season.sky_hue, new_season.sky_hue, new_colors);
        let sky_sat = lerp(old_season.sky_sat, new_season.sky_sat, new_colors);

        // Flakes land on whichever pile is higher.
        let surface_y = new_pile.top_y.min(old_pile.top_y);
        let flakes: [Flake; FLAKES] = core::array::from_fn(|slot| {
            make_flake(
                slot, elapsed_ms, self.seed, surface_y, old_season, new_season, new_colors,
            )
        });
        self.mark_flakes(&flakes);

        for (i, led) in LED_MAP.iter().enumerate() {
            // Per-LED texture and pulse, shared by both piles.
            let hash = self.led_hash[i];
            let wave = pulse_phase.sin_plus(self.pulse_offset[i]);
            let pulse = 1.0 - PILE_PULSE * (0.5 - 0.5 * wave);
            let pile_val = self.pile_brightness[i] * pulse;

            // How far inside each pile this LED is; the jitter shrinks to 0 at empty and full.
            let jitter = self.jitter_mm[i];
            let inside_new = new_pile.depth(led.wy, jitter);
            let inside_old = old_pile.depth(led.wy, jitter);

            // Sky, then the old pile over it, then the new pile on top.
            let (old_hue, old_sat) = old_season.color(hash);
            let (new_hue, new_sat) = new_season.color(hash);
            let mut hue = sky_hue;
            let mut sat = sky_sat;
            let mut val = SKY_VAL;
            (hue, sat) = blend_color(hue, sat, old_hue, old_sat, inside_old);
            val = lerp(val, pile_val, inside_old);
            (hue, sat) = blend_color(hue, sat, new_hue, new_sat, inside_new);
            val = lerp(val, pile_val, inside_new);

            let flake = self.flake_hits[i];
            if flake.strength > 0.0 {
                (hue, sat) = blend_color(hue, sat, flake.hue, flake.sat, flake.strength);
                val += (1.0 - val) * (flake.strength * FLAKE_VAL);
            }

            out[i] = hsv(wrap360(hue), sat, val);
        }
    }
}

impl Fubuki {
    /// Keeps the strongest flake per LED.
    fn mark_flakes(&mut self, flakes: &[Flake]) {
        for hit in &mut self.flake_hits {
            hit.strength = 0.0;
        }
        for flake in flakes {
            grid::for_each_near(flake.x, flake.y, FLAKE_R_MM, |i| {
                let led = &LED_MAP[i];
                let dx = led.wx - flake.x;
                let dy = led.wy - flake.y;
                let strength = 1.0 - (dx * dx + dy * dy) / (FLAKE_R_MM * FLAKE_R_MM);
                if strength > self.flake_hits[i].strength {
                    self.flake_hits[i] = FlakeHit {
                        strength,
                        hue: flake.hue,
                        sat: flake.sat,
                    };
                }
            });
        }
    }
}

/// Flake `slot`'s position and color this frame, from time and hashes.
fn make_flake(
    slot: usize,
    t_ms: u32,
    seed: u32,
    surface_y: f32,
    old: &Season,
    new: &Season,
    new_colors: f32,
) -> Flake {
    // Each slot has its own fall time and start offset, so the flakes do not drop together.
    // `fall` is 0 at the top, 1 at the pile.
    let (flake_num, fall) = slot_life(slot as u32 + 1, FLAKE_SALT, t_ms, FALL_MIN_MS, FALL_MAX_MS);

    let hash = hash2(slot as u32 + 1, flake_num ^ seed);
    // Across the fixture, a little in from each side.
    let spawn_x = lerp(
        WORLD_LEFT + START_INSET_LEFT_MM,
        WORLD_RIGHT - START_INSET_RIGHT_MM,
        hash01(hash, 1),
    );
    let sway_phase = hash01(hash, 2) * TAU;

    let x = lerp(spawn_x, WORLD_CX, fall * CONVERGE)
        + SWAY_MM * (fall * SWAY_CYCLES * TAU + sway_phase).sin();
    let y = WORLD_TOP + fall * (surface_y - WORLD_TOP);
    // A fraction `new_colors` of the flakes use the new season's colors.
    let season = if hash01(hash, 3) < new_colors {
        new
    } else {
        old
    };
    let (hue, sat) = season.color(hash);

    Flake { x, y, hue, sat }
}

/// World y of the pile's top edge for a fill fraction (0 empty, 1 full).
fn pile_top_y(fill: f32) -> f32 {
    let margin = FILL_FEATHER_MM;
    (WORLD_BOT + margin) - (WORLD_H + 2.0 * margin) * fill.powf(FILL_EXP)
}

/// Blends a color toward a target by `t`. White has no hue, so the other color's is used.
fn blend_color(hue: f32, sat: f32, to_hue: f32, to_sat: f32, t: f32) -> (f32, f32) {
    let hue = if to_sat == 0.0 {
        hue
    } else if sat == 0.0 {
        to_hue
    } else {
        blend_hue(hue, to_hue, t)
    };
    (hue, lerp(sat, to_sat, t))
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
