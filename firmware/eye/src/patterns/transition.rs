use std::sync::OnceLock;

use core::f32::consts::TAU;

use crate::led::geom::{BOARD_CENTER, BOARD_COUNT, DIST_C, DIST_C_MAX};
use crate::led::map::{LED_MAP, WORLD_CENTROID_X, WORLD_CENTROID_Y};
use crate::patterns::{hash01, lerp, smoothstep, Frame};

const FEATHER_MM: f32 = 60.0; // width of the soft edge on radial wipes
const SPARKLE_EDGE: f32 = 0.15; // time each LED takes to change, as a fraction of progress

const SPIRAL_TURNS: f32 = 1.5; // turns the spiral makes from center to edge
const SPIRAL_EDGE: f32 = 0.12; // time each tile takes to change, as a fraction of progress

#[derive(Clone, Copy)]
pub enum TransitionStyle {
    /// The whole frame fades at once.
    Crossfade,
    /// From the center outward.
    RadialOut,
    /// From the edges inward.
    RadialIn,
    /// Each LED changes at its own random moment.
    Sparkle,
    /// From the center outward, with a ragged edge.
    RadialSparkle,
    /// Tile by tile, as a spiral arm sweeps around the fixture.
    SpiralOut,
    /// The same sweep in reverse.
    SpiralIn,
}

/// Blends `from` into `out`, which holds the incoming frame on entry. `progress` is 0-1.
pub fn blend(style: TransitionStyle, progress: f32, from: &Frame, out: &mut Frame) {
    let ranks = board_spiral_ranks();
    for (i, led) in LED_MAP.iter().enumerate() {
        let alpha = alpha_for(style, i, progress, ranks[led.board_id as usize]);
        out[i] = lerp_rgb(from[i], out[i], alpha);
    }
}

/// Mix for one LED: 0 is the outgoing frame, 1 the incoming. `i` is the LED's index.
fn alpha_for(style: TransitionStyle, i: usize, progress: f32, board_rank: f32) -> f32 {
    let dist_c = DIST_C[i];
    match style {
        TransitionStyle::Crossfade => smoothstep(progress),
        TransitionStyle::RadialOut => {
            let front = progress * (DIST_C_MAX + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((front - dist_c) / FEATHER_MM)
        }
        TransitionStyle::RadialIn => {
            let front = (1.0 - progress) * (DIST_C_MAX + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((dist_c - front) / FEATHER_MM)
        }
        TransitionStyle::Sparkle => {
            let threshold = hash01(i as u32, 0) * (1.0 - SPARKLE_EDGE);
            smoothstep((progress - threshold) / SPARKLE_EDGE)
        }
        TransitionStyle::RadialSparkle => {
            let radial = dist_c / DIST_C_MAX;
            let threshold = (0.5 * radial + 0.5 * hash01(i as u32, 0)) * (1.0 - SPARKLE_EDGE);
            smoothstep((progress - threshold) / SPARKLE_EDGE)
        }
        TransitionStyle::SpiralOut => {
            let threshold = board_rank * (1.0 - SPIRAL_EDGE);
            smoothstep((progress - threshold) / SPIRAL_EDGE)
        }
        TransitionStyle::SpiralIn => {
            let threshold = (1.0 - board_rank) * (1.0 - SPIRAL_EDGE);
            smoothstep((progress - threshold) / SPIRAL_EDGE)
        }
    }
}

/// Each tile's place in the spiral order by `board_id`, 0 (first) to 1 (last).
fn board_spiral_ranks() -> &'static [f32; BOARD_COUNT + 1] {
    static RANKS: OnceLock<[f32; BOARD_COUNT + 1]> = OnceLock::new();
    RANKS.get_or_init(compute_spiral_ranks)
}

fn compute_spiral_ranks() -> [f32; BOARD_COUNT + 1] {
    let mut polar: Vec<(usize, f32, f32)> = Vec::new(); // (board_id, radius, angle)
    let mut r_min = f32::MAX;
    let mut r_max = 0.0f32;
    for (board, &(x, y)) in BOARD_CENTER.iter().enumerate().skip(1) {
        let dx = x - WORLD_CENTROID_X;
        let dy = y - WORLD_CENTROID_Y;
        let radius = (dx * dx + dy * dy).sqrt();
        r_min = r_min.min(radius);
        r_max = r_max.max(radius);
        polar.push((board, radius, dy.atan2(dx)));
    }

    // Sort by angle plus a term that grows with radius, which orders the tiles along a spiral.
    let span = (r_max - r_min).max(1.0);
    let mut keyed: Vec<(usize, f32)> = polar
        .iter()
        .map(|&(board, radius, angle)| {
            (board, angle + (radius - r_min) / span * SPIRAL_TURNS * TAU)
        })
        .collect();
    keyed.sort_by(|a, b| a.1.total_cmp(&b.1));

    let mut ranks = [0.0f32; BOARD_COUNT + 1];
    let last = (keyed.len().max(2) - 1) as f32;
    for (pos, &(board, _)) in keyed.iter().enumerate() {
        ranks[board] = pos as f32 / last;
    }
    ranks
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    core::array::from_fn(|c| lerp(a[c] as f32, b[c] as f32, t).clamp(0.0, 255.0) as u8)
}
