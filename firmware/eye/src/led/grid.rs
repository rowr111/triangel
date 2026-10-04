use super::geom::{CELL_LEDS, CELL_START};

// LEDs grouped into square cells, for finding the ones near a point. The cell tables in
// geom.rs come from tools/gen_geom.py; rerun it if the constants below change.

pub const CELL_MM: f32 = 48.0;
pub const COLS: usize = 12;
pub const ROWS: usize = 10;

fn col(x: f32) -> usize {
    (x / CELL_MM).clamp(0.0, (COLS - 1) as f32) as usize
}

fn row(y: f32) -> usize {
    (y / CELL_MM).clamp(0.0, (ROWS - 1) as f32) as usize
}

fn for_each_in_cells(gx0: usize, gx1: usize, gy0: usize, gy1: usize, f: &mut impl FnMut(usize)) {
    for gy in gy0..=gy1 {
        for gx in gx0..=gx1 {
            let cell = gy * COLS + gx;
            let run = CELL_START[cell] as usize..CELL_START[cell + 1] as usize;
            for &li in &CELL_LEDS[run] {
                f(li as usize);
            }
        }
    }
}

/// Calls `f` with each LED index within `reach` cells of (x, y).
pub fn for_each_near(x: f32, y: f32, reach: usize, f: impl FnMut(usize)) {
    for_each_near_seg((x, y), (x, y), reach, f);
}

/// Calls `f` with each LED index in the box of cells around segment a-b, widened by
/// `reach`. That is more than the LEDs near the segment, so callers still check distance.
pub fn for_each_near_seg(a: (f32, f32), b: (f32, f32), reach: usize, mut f: impl FnMut(usize)) {
    let (cx0, cx1) = (col(a.0.min(b.0)), col(a.0.max(b.0)));
    let (cy0, cy1) = (row(a.1.min(b.1)), row(a.1.max(b.1)));
    for_each_in_cells(
        cx0.saturating_sub(reach),
        (cx1 + reach).min(COLS - 1),
        cy0.saturating_sub(reach),
        (cy1 + reach).min(ROWS - 1),
        &mut f,
    );
}
