use super::geom::{CELL_LEDS, CELL_START};

// Spatial index over the fixture: LED indices bucketed into fixed cells, so a dot can splat
// onto the LEDs near it instead of every LED testing itself against every dot. The buckets
// themselves are generated into geom.rs by tools/gen_geom.py.

pub const CELL_MM: f32 = 48.0;
pub const COLS: usize = 12;
pub const ROWS: usize = 10;

/// Call `f` with each LED index within `reach` cells of the world point (x, y). `reach` must
/// cover the dot's radius, or LEDs it touches will be missed.
pub fn for_each_near(x: f32, y: f32, reach: usize, mut f: impl FnMut(usize)) {
    let cx = (x / CELL_MM).clamp(0.0, (COLS - 1) as f32) as usize;
    let cy = (y / CELL_MM).clamp(0.0, (ROWS - 1) as f32) as usize;
    for gy in cy.saturating_sub(reach)..=(cy + reach).min(ROWS - 1) {
        for gx in cx.saturating_sub(reach)..=(cx + reach).min(COLS - 1) {
            let cell = gy * COLS + gx;
            let run = CELL_START[cell] as usize..CELL_START[cell + 1] as usize;
            for &li in &CELL_LEDS[run] {
                f(li as usize);
            }
        }
    }
}
