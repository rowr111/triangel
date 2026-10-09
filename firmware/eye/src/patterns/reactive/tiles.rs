use crate::audio::Audio;
use crate::led::geom::{BOARD_CENTER, BOARD_COUNT};
use crate::led::map::{LED_MAP, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use crate::patterns::{Frame, Pattern, Rng, Shot, hsv};
use triangel_shared::tuning::tiles::*;

/// Tiles are numbered by board id, from 1. Index 0 is unused.
const TILES: usize = BOARD_COUNT + 1;
/// Tile centers are 60 mm apart across a shared edge and 103 mm across a shared corner.
const NEIGHBOR_MM: f32 = 75.0;
const MAX_NEIGHBORS: usize = 3;

/// (hue, sat). The cooler hues are less saturated, or they would look dim.
const PALETTE: [(f32, f32); 5] =
    [(275.0, 0.75), (200.0, 0.80), (330.0, 0.88), (40.0, 0.95), (160.0, 0.82)];

/// Whole tiles light on the beat. A light beat lights one tile, a harder one spreads to
/// its neighbors ring by ring, and a drop spreads from the middle to every tile.
pub struct Tiles {
    rng:         Rng,
    neighbors:   [[u8; MAX_NEIGHBORS]; TILES],
    n_neighbors: [u8; TILES],
    /// The tile nearest the middle of the fixture, where a drop starts.
    center:      usize,
    /// Each tile's current flash. Outer rings start a little in the future.
    shots:       [Shot; TILES],
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
            shots: [Shot::NONE; TILES],
        }
    }

    /// Lights `count` tiles outward from `seed`, each ring RING_STEP_MS after the last. A
    /// tile whose current flash would outlast the new one keeps it, so beats do not cut a
    /// drop's long fade short.
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
            let cur = &mut self.shots[t];
            let outlasts = cur.strength > 0.0 && cur.end_ms().wrapping_sub(new.end_ms()) as i32 > 0;
            if !outlasts {
                *cur = new;
            }
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
            let s = &self.shots[t];
            let Some((age, fade)) = s.level(t_ms) else { continue };
            let young = (1.0 - age / ((s.hold_ms + s.fade_ms) * WHITE_FRAC)).clamp(0.0, 1.0);
            let v = (s.strength * fade * fade * (1.0 + HIT_BOOST * young)).min(1.0);
            *c = hsv(s.hue, s.sat * (1.0 - young), v);
        }

        for (o, led) in out.iter_mut().zip(LED_MAP.iter()) {
            *o = color[led.board_id as usize];
        }
    }
}
