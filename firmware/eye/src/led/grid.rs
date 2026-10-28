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

fn for_each_in_cells(
    left_col: usize,
    right_col: usize,
    top_row: usize,
    bottom_row: usize,
    f: &mut impl FnMut(usize),
) {
    for row in top_row..=bottom_row {
        for col in left_col..=right_col {
            let cell = row * COLS + col;
            let run = CELL_START[cell] as usize..CELL_START[cell + 1] as usize;
            for &led in &CELL_LEDS[run] {
                f(led as usize);
            }
        }
    }
}

/// Calls `f` with each LED index in the cells within `radius_mm` of (x, y).
pub fn for_each_near(x: f32, y: f32, radius_mm: f32, f: impl FnMut(usize)) {
    for_each_near_seg((x, y), (x, y), radius_mm, f);
}

/// Calls `f` with each LED index in the box of cells around segment a-b, widened by
/// `radius_mm`. That is more than the LEDs near the segment, so callers still check distance.
pub fn for_each_near_seg(a: (f32, f32), b: (f32, f32), radius_mm: f32, mut f: impl FnMut(usize)) {
    let reach = (radius_mm / CELL_MM).ceil() as usize;
    let (left_col, right_col) = (col(a.0.min(b.0)), col(a.0.max(b.0)));
    let (top_row, bottom_row) = (row(a.1.min(b.1)), row(a.1.max(b.1)));
    for_each_in_cells(
        left_col.saturating_sub(reach),
        (right_col + reach).min(COLS - 1),
        top_row.saturating_sub(reach),
        (bottom_row + reach).min(ROWS - 1),
        &mut f,
    );
}
