use std::sync::OnceLock;

use core::f32::consts::TAU;

use super::Frame;
use crate::led::map::{Led, WORLD_CENTROID_X, WORLD_CENTROID_Y};

// Triangle centroid, from the shared world constants.
const CENTER_X: f32 = WORLD_CENTROID_X;
const CENTER_Y: f32 = WORLD_CENTROID_Y;

// Width of the soft reveal band for radial wipes, and the soft edge for sparkle (as a
// fraction of progress). Both just shape how hard/soft the transition front looks.
const FEATHER_MM: f32 = 60.0;
const SPARKLE_EDGE: f32 = 0.15;

// Spiral wipe: how many turns the arm winds from center to edge (tightness), and how many
// tiles are mid-transition at once (soft per-board edge, as a fraction of progress).
const SPIRAL_TURNS: f32 = 1.5;
const SPIRAL_EDGE: f32 = 0.12;

#[derive(Clone, Copy)]
pub enum TransitionStyle {
    Crossfade,     // uniform fade of the whole frame, outgoing -> incoming
    RadialOut,     // blooms from the centroid outward
    RadialIn,      // collapses from the edges into the centroid
    Sparkle,       // each LED crosses at its own random moment
    RadialSparkle, // radial front, but raggedy/dissolving rather than a clean ring
    SpiralOut,     // one triangle at a time, spiraling out from the center
    SpiralIn,      // one triangle at a time, spiraling in toward the center
}

/// Blend the outgoing frame into the incoming one. `out` holds the incoming (target)
/// frame on entry; on return it holds the composited result. `progress` is 0.0->1.0.
pub fn blend(style: TransitionStyle, leds: &[Led], progress: f32, from: &Frame, out: &mut Frame) {
    let maxd = max_center_dist(leds);
    let ranks = board_spiral_ranks(leds);
    for (i, led) in leds.iter().enumerate() {
        let alpha = alpha_for(style, led, progress, maxd, ranks[led.board_id as usize]);
        out[i] = lerp_rgb(from[i], out[i], alpha);
    }
}

/// Per-LED mix factor: 0.0 = fully outgoing, 1.0 = fully incoming.
fn alpha_for(style: TransitionStyle, led: &Led, progress: f32, maxd: f32, board_rank: f32) -> f32 {
    match style {
        TransitionStyle::Crossfade => smoothstep(progress),
        TransitionStyle::RadialOut => {
            let front = progress * (maxd + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((front - dist_to_center(led)) / FEATHER_MM)
        }
        TransitionStyle::RadialIn => {
            let front = (1.0 - progress) * (maxd + 2.0 * FEATHER_MM) - FEATHER_MM;
            smoothstep((dist_to_center(led) - front) / FEATHER_MM)
        }
        TransitionStyle::Sparkle => {
            let threshold = hash01(led) * (1.0 - SPARKLE_EDGE);
            smoothstep((progress - threshold) / SPARKLE_EDGE)
        }
        TransitionStyle::RadialSparkle => {
            let radial = dist_to_center(led) / maxd;
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

fn dist_to_center(led: &Led) -> f32 {
    led.dist_to(CENTER_X, CENTER_Y)
}

/// Farthest LED distance from the centroid, computed once (geometry is fixed). Used to
/// normalize the radial wipes so they always complete regardless of where the center is.
fn max_center_dist(leds: &[Led]) -> f32 {
    static MAX: OnceLock<f32> = OnceLock::new();
    *MAX.get_or_init(|| leds.iter().map(dist_to_center).fold(0.0_f32, f32::max))
}

/// Per-board switch order for the spiral wipe, indexed by `board_id` (1..=25), each a
/// normalized rank in [0, 1] (0 = first/innermost, 1 = last/outermost). Computed once.
/// All LEDs on a board share its rank, so a whole triangle flips together.
fn board_spiral_ranks(leds: &[Led]) -> &'static [f32; 26] {
    static RANKS: OnceLock<[f32; 26]> = OnceLock::new();
    RANKS.get_or_init(|| compute_spiral_ranks(leds))
}

fn compute_spiral_ranks(leds: &[Led]) -> [f32; 26] {
    // Accumulate each board's centroid from its LEDs.
    let mut sx = [0.0f32; 26];
    let mut sy = [0.0f32; 26];
    let mut n  = [0u32; 26];
    for led in leds {
        let b = led.board_id as usize;
        sx[b] += led.wx;
        sy[b] += led.wy;
        n[b]  += 1;
    }

    // Per board: radius + angle from the design centroid. Track the radius span so we can
    // normalize it for the spiral key.
    let mut polar: Vec<(usize, f32, f32)> = Vec::new(); // (board_id, radius, angle)
    let mut rmin = f32::MAX;
    let mut rmax = 0.0f32;
    for b in 1..26 {
        if n[b] == 0 {
            continue;
        }
        let dx = sx[b] / n[b] as f32 - CENTER_X;
        let dy = sy[b] / n[b] as f32 - CENTER_Y;
        let r = (dx * dx + dy * dy).sqrt();
        rmin = rmin.min(r);
        rmax = rmax.max(r);
        polar.push((b, r, dy.atan2(dx)));
    }

    // Spiral key: angle plus a radius term so the arm both rotates and grows outward.
    let span = (rmax - rmin).max(1.0);
    let mut keyed: Vec<(usize, f32)> = polar
        .iter()
        .map(|&(b, r, a)| (b, a + (r - rmin) / span * SPIRAL_TURNS * TAU))
        .collect();
    keyed.sort_by(|x, y| x.1.partial_cmp(&y.1).unwrap_or(core::cmp::Ordering::Equal));

    // Normalize sorted position to a [0, 1] rank.
    let mut ranks = [0.0f32; 26];
    let last = (keyed.len().max(2) - 1) as f32;
    for (pos, &(b, _)) in keyed.iter().enumerate() {
        ranks[b] = pos as f32 / last;
    }
    ranks
}

/// Deterministic per-LED value in [0, 1), spread by a cheap hash of the chain index.
fn hash01(led: &Led) -> f32 {
    let h = (led.chain_idx as u32).wrapping_mul(2654435761) ^ 0x9E37_79B9;
    (h % 1000) as f32 / 1000.0
}

fn smoothstep(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn lerp_rgb(a: [u8; 3], b: [u8; 3], t: f32) -> [u8; 3] {
    [lerp_u8(a[0], b[0], t), lerp_u8(a[1], b[1], t), lerp_u8(a[2], b[2], t)]
}

fn lerp_u8(a: u8, b: u8, t: f32) -> u8 {
    (a as f32 + (b as f32 - a as f32) * t).clamp(0.0, 255.0) as u8
}
