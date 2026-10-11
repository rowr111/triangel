use crate::audio::Audio;
use crate::led::geom::{BOARD_CENTER, BOARD_COUNT, TILE_SPACING_MM};
use crate::led::map::{LED_MAP, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use crate::patterns::{Frame, Pattern, Rng, Shot, hsv};
use triangel_shared::tuning::tiles::*;

/// Tiles are numbered by board id, from 1. Index 0 is unused.
const TILES: usize = BOARD_COUNT + 1;
/// Tiles sharing an edge are TILE_SPACING_MM apart; sharing only a corner, 1.7 times that.
const NEIGHBOR_MM: f32 = TILE_SPACING_MM * 1.25;
const MAX_NEIGHBORS: usize = 3;

/// (hue, sat). Cooler hues are less saturated, or they look dim.
const PALETTE: [(f32, f32); 5] =
    [(275.0, 0.75), (200.0, 0.80), (330.0, 0.88), (40.0, 0.95), (160.0, 0.82)];

/// Whole tiles light on the beat: one tile, spreading to neighbors on a harder beat, and
/// to every tile from the middle on a drop.
pub struct Tiles {
    rng:         Rng,
    neighbors:   [[u8; MAX_NEIGHBORS]; TILES],
    n_neighbors: [u8; TILES],
    /// The middle tile, where a drop starts.
    center:      usize,
    /// Each tile's two latest flashes; the brighter shows.
    shots:       [[Shot; 2]; TILES],
}

impl Tiles {
    pub fn new() -> Self {
        let centers = &BOARD_CENTER;
        let mut neighbors = [[0u8; MAX_NEIGHBORS]; TILES];
        let mut n_neighbors = [0u8; TILES];
        let near2 = NEIGHBOR_MM * NEIGHBOR_MM;
        for a in 1..TILES {
            for b in 1..TILES {
                let (dx, dy) = (centers[a].0 - centers[b].0, centers[a].1 - centers[b].1);
                let n = n_neighbors[a] as usize;
                if a != b && dx * dx + dy * dy < near2 && n < MAX_NEIGHBORS {
                    neighbors[a][n] = b as u8;
                    n_neighbors[a] += 1;
                }
            }
        }

        let center = (1..TILES)
            .min_by(|&a, &b| {
                let da = (centers[a].0 - WORLD_CENTROID_X).powi(2)
                    + (centers[a].1 - WORLD_CENTROID_Y).powi(2);
                let db = (centers[b].0 - WORLD_CENTROID_X).powi(2)
                    + (centers[b].1 - WORLD_CENTROID_Y).powi(2);
                da.total_cmp(&db)
            })
            .unwrap_or(1);

        Tiles {
            rng: Rng::new(0x5EED_1234),
            neighbors,
            n_neighbors,
            center,
            shots: [[Shot::NONE; 2]; TILES],
        }
    }

    /// Lights `count` tiles outward from `seed`, each ring RING_STEP_MS after the last.
    fn light(&mut self, seed: usize, t_ms: u32, count: usize, strength: f32, hold: f32, fade: f32) {
        let (hue, sat) = self.rng.pick(&PALETTE);
        let mut ring = [u8::MAX; TILES];
        let mut queue = [0u8; TILES];
        let (mut head, mut tail) = (0, 1);
        queue[0] = seed as u8;
        ring[seed] = 0;
        let mut lit = 0;
        while head < tail && lit < count {
            let t = queue[head] as usize;
            head += 1;
            let start_ms = t_ms.wrapping_add(ring[t] as u32 * RING_STEP_MS);
            let new = Shot { start_ms, strength, hue, sat, hold_ms: hold, fade_ms: fade };
            let new_left = new.end_ms().wrapping_sub(t_ms) as i32;
            // Replace the flash that ends sooner, but a waiting one before a showing one,
            // so the tile does not go dark.
            let remaining = |s: &Shot| {
                let left = s.end_ms().wrapping_sub(t_ms) as i32;
                let waiting = s.start_ms.wrapping_sub(t_ms) as i32 > 0;
                if s.strength <= 0.0 {
                    i32::MIN
                } else if waiting && left <= new_left {
                    i32::MIN + 1
                } else {
                    left
                }
            };
            let pair = &mut self.shots[t];
            let slot = if remaining(&pair[0]) <= remaining(&pair[1]) { 0 } else { 1 };
            pair[slot] = new;
            lit += 1;
            for &n in &self.neighbors[t][..self.n_neighbors[t] as usize] {
                let n = n as usize;
                if ring[n] == u8::MAX {
                    ring[n] = ring[t] + 1;
                    queue[tail] = n as u8;
                    tail += 1;
                }
            }
        }
    }
}

impl Pattern for Tiles {
    fn on_enter(&mut self, _t_ms: u32) {
        self.shots = [[Shot::NONE; 2]; TILES];
    }

    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            self.light(self.center, t_ms, TILES - 1, 1.0, DROP_HOLD_MS, DROP_FADE_MS);
        } else if audio.beat {
            let seed = 1 + self.rng.below(TILES - 1);
            let count = 1 + (audio.beat_strength * GROW_EXTRA) as usize;
            self.light(seed, t_ms, count, 0.5 + 0.5 * audio.beat_strength, 0.0, FADE_MS);
        }

        // One color per tile, shared by all its LEDs.
        let mut color = [[0u8; 3]; TILES];
        for (t, c) in color.iter_mut().enumerate().skip(1) {
            let mut best = 0.0;
            for s in self.shots[t].iter_mut() {
                // Forget a finished flash; its end time goes stale.
                if s.strength > 0.0 && t_ms.wrapping_sub(s.end_ms()) as i32 >= 0 {
                    s.strength = 0.0;
                }
                let white_ms = (s.hold_ms + s.fade_ms) * WHITE_FRAC;
                let Some((v, sat)) = s.lit(t_ms, white_ms, HIT_BOOST) else { continue };
                if v > best {
                    best = v;
                    *c = hsv(s.hue, sat, v);
                }
            }
        }

        for (o, led) in out.iter_mut().zip(LED_MAP.iter()) {
            *o = color[led.board_id as usize];
        }
    }
}
