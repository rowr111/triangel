use std::sync::OnceLock;

use core::f32::consts::TAU;

use super::{Frame, knuth_hash, smoothstep};
use crate::led::geom::{BOARD_CENTER, BOARD_COUNT, DIST_C, DIST_C_MAX};
use crate::led::map::{Led, LED_MAP, WORLD_CENTROID_X, WORLD_CENTROID_Y};

const CENTER_X: f32 = WORLD_CENTROID_X;
const CENTER_Y: f32 = WORLD_CENTROID_Y;

const FEATHER_MM: f32 = 60.0;   // width of the soft edge on radial wipes
const SPARKLE_EDGE: f32 = 0.15; // time each LED takes to change, as a fraction of progress

const SPIRAL_TURNS: f32 = 1.5; // turns the spiral makes from center to edge
const SPIRAL_EDGE: f32 = 0.12; // time each tile takes to change, as a fraction of progress

#[derive(Clone, Copy)]
pub enum TransitionStyle {
    Crossfade,     // the whole frame fades at once
    RadialOut,     // from the center outward
    RadialIn,      // from the edges inward
    Sparkle,       // each LED changes at its own random moment
    RadialSparkle, // from the center outward, with a ragged edge
    SpiralOut,     // one tile at a time, spiraling out from the center
    SpiralIn,      // one tile at a time, spiraling in to the center
}

/// Blends `from` into `out`, which holds the incoming frame on entry. `progress` is 0-1.
pub fn blend(style: TransitionStyle, progress: f32, from: &Frame, out: &mut Frame) {
    let ranks = board_spiral_ranks();
    for (i, led) in LED_MAP.iter().enumerate() {
        let alpha = alpha_for(style, led, DIST_C[i], progress, DIST_C_MAX, ranks[led.board_id as usize]);
        out[i] = lerp_rgb(from[i], out[i], alpha);
    }
}

/// Mix for one LED: 0 is the outgoing frame, 1 the incoming. `dist_c` is the LED's
/// distance from the center.
fn alpha_for(
    style: TransitionStyle,
    led: &Led,
    dist_c: f32,
    progress: f32,
    maxd: f32,
    board_rank: f32,
) -> f32 {
    match style {
        TransitionStyle::Crossfade => smoothstep(progress),
        TransitionStyle::RadialOut => {
            let front = progress * (maxd + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((front - dist_c) / FEATHER_MM)
        }
        TransitionStyle::RadialIn => {
            let front = (1.0 - progress) * (maxd + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((dist_c - front) / FEATHER_MM)
        }
        TransitionStyle::Sparkle => {
            let threshold = hash01(led) * (1.0 - SPARKLE_EDGE);
            smoothstep((progress - threshold) / SPARKLE_EDGE)
        }
        TransitionStyle::RadialSparkle => {
            let radial = dist_c / maxd;
            let threshold = (0.5 * radial + 0.5 * hash01(led)) * (1.0 - SPARKLE_EDGE);
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

/// Each tile's place in the spiral order by `board_id`, 0 (first, innermost) to 1 (last).
fn board_spiral_ranks() -> &'static [f32; BOARD_COUNT + 1] {
    static RANKS: OnceLock<[f32; BOARD_COUNT + 1]> = OnceLock::new();
    RANKS.get_or_init(compute_spiral_ranks)
}

fn compute_spiral_ranks() -> [f32; BOARD_COUNT + 1] {
    let mut polar: Vec<(usize, f32, f32)> = Vec::new(); // (board_id, radius, angle)
    let mut rmin = f32::MAX;
    let mut rmax = 0.0f32;
    for (b, &(x, y)) in BOARD_CENTER.iter().enumerate().skip(1) {
        let dx = x - CENTER_X;
        let dy = y - CENTER_Y;
        let r = (dx * dx + dy * dy).sqrt();
        rmin = rmin.min(r);
        rmax = rmax.max(r);
        polar.push((b, r, dy.atan2(dx)));
    }

    // Sort by angle plus a term that grows with radius, which orders the tiles along a spiral.
    let span = (rmax - rmin).max(1.0);
    let mut keyed: Vec<(usize, f32)> = polar
        .iter()
        .map(|&(b, r, a)| (b, a + (r - rmin) / span * SPIRAL_TURNS * TAU))
        .collect();
    keyed.sort_by(|x, y| x.1.total_cmp(&y.1));

    let mut ranks = [0.0f32; BOARD_COUNT + 1];
    let last = (keyed.len().max(2) - 1) as f32;
    for (pos, &(b, _)) in keyed.iter().enumerate() {
        ranks[b] = pos as f32 / last;
    }
    ranks
}

/// Per-LED value in [0, 1) from a hash of the chain index.
fn hash01(led: &Led) -> f32 {
    let h = knuth_hash(led.chain_idx as u32) ^ 0x9E37_79B9;
    (h % 1000) as f32 / 1000.0
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [lerp_u8(a[0], b[0], t), lerp_u8(a[1], b[1], t), lerp_u8(a[2], b[2], t)]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}
