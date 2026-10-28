pub use super::geom::{WORLD_HALF_W, WORLD_LEFT, WORLD_RIGHT};
use super::map::{WORLD_BOT, WORLD_H};

/// Half the width of the triangle at `y`, in mm.
pub fn half_width_at(y: f32) -> f32 {
    (WORLD_BOT - y) / WORLD_H * WORLD_HALF_W
}
