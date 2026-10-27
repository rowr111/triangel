use std::collections::VecDeque;

use crate::audio::Audio;
use crate::led::geom::{BOARD_CENTER, BOARD_COUNT, TILE_SPACING_MM};
use crate::led::map::{LED_MAP, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use crate::patterns::{hsv, Frame, Pattern, Rng, Shot, ShotPair};
use triangel_shared::tuning::tiles::*;

/// Tiles are numbered by board id, from 1. Index 0 is unused.
const TILES: usize = BOARD_COUNT + 1;
/// Tiles sharing an edge are TILE_SPACING_MM apart; sharing only a corner, 1.7 times that.
const NEIGHBOR_MM: f32 = TILE_SPACING_MM * 1.25;

/// (hue, sat). Cooler hues are less saturated, or they look dim.
const PALETTE: [(f32, f32); 5] = [
    (275.0, 0.75),
    (200.0, 0.80),
    (330.0, 0.88),
    (40.0, 0.95),
    (160.0, 0.82),
];

/// Whole tiles light on the beat: one tile, spreading to neighbors on a harder beat, and
/// to every tile from the middle on a drop.
pub struct Tiles {
    rng: Rng,
    neighbors: [Vec<usize>; TILES],
    /// The middle tile, where a drop starts.
    center: usize,
    /// Each tile's two latest flashes; the brighter shows.
    shots: [ShotPair; TILES],
}

impl Tiles {
    pub fn new() -> Self {
        let dist2 = |a: (f32, f32), b: (f32, f32)| {
            let (dx, dy) = (a.0 - b.0, a.1 - b.1);
            dx * dx + dy * dy
        };

        let near2 = NEIGHBOR_MM * NEIGHBOR_MM;
        let mut neighbors: [Vec<usize>; TILES] = core::array::from_fn(|_| Vec::new());
        for (a, list) in neighbors.iter_mut().enumerate().skip(1) {
            *list = (1..TILES)
                .filter(|&b| b != a && dist2(BOARD_CENTER[a], BOARD_CENTER[b]) < near2)
                .collect();
        }

        let middle = (WORLD_CENTROID_X, WORLD_CENTROID_Y);
        let from_middle = |tile: usize| dist2(BOARD_CENTER[tile], middle);
        let center = (1..TILES)
            .min_by(|&a, &b| from_middle(a).total_cmp(&from_middle(b)))
            .unwrap_or(1);

        Tiles {
            rng: Rng::new(0x5EED_1234),
            neighbors,
            center,
            shots: [ShotPair::NONE; TILES],
        }
    }
}

impl Pattern for Tiles {
    fn on_enter(&mut self, _t_ms: u32) {
        self.shots = [ShotPair::NONE; TILES];
    }

    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            self.light(
                self.center,
                t_ms,
                BOARD_COUNT,
                1.0,
                DROP_HOLD_MS,
                DROP_FADE_MS,
            );
        } else if audio.beat {
            let seed = 1 + self.rng.below(BOARD_COUNT);
            let count = 1 + (audio.beat_strength * GROW_EXTRA) as usize;
            let strength = 0.5 + 0.5 * audio.beat_strength;
            self.light(seed, t_ms, count, strength, 0.0, FADE_MS);
        }

        // One color per tile, shared by all its LEDs.
        let mut color = [[0u8; 3]; TILES];
        for (tile, c) in color.iter_mut().enumerate().skip(1) {
            if let Some(shot) = self.shots[tile].brightest(t_ms) {
                *c = hsv(shot.hue, shot.sat, shot.brightness);
            }
        }

        for (o, led) in out.iter_mut().zip(LED_MAP.iter()) {
            *o = color[led.board_id as usize];
        }
    }
}

impl Tiles {
    /// Lights `count` tiles outward from `seed`, each ring RING_STEP_MS after the last.
    fn light(&mut self, seed: usize, t_ms: u32, count: usize, strength: f32, hold: f32, fade: f32) {
        let (hue, sat) = self.rng.pick(&PALETTE);
        let mut queued = [false; TILES];
        queued[seed] = true;
        // Tiles waiting to light, with how many rings out from `seed` each one is.
        let mut queue = VecDeque::from([(seed, 0)]);
        for _ in 0..count {
            let Some((tile, ring)) = queue.pop_front() else {
                break;
            };
            let shot = Shot {
                start_ms: t_ms.wrapping_add(ring * RING_STEP_MS),
                strength,
                hue,
                sat,
                hold_ms: hold,
                fade_ms: fade,
                white_ms: (hold + fade) * WHITE_FRAC,
                boost: HIT_BOOST,
            };
            self.shots[tile].add(shot, t_ms);
            for &next in &self.neighbors[tile] {
                if !queued[next] {
                    queued[next] = true;
                    queue.push_back((next, ring + 1));
                }
            }
        }
    }
}
