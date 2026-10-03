use super::map::WORLD_CX;

// The fixture is an equilateral point-down triangle, so its top edge reaches about
// WORLD_H / sqrt(3) either side of center.
pub const WORLD_HALF_W: f32 = 248.0;
pub const WORLD_LEFT:   f32 = WORLD_CX - WORLD_HALF_W;
pub const WORLD_RIGHT:  f32 = WORLD_CX + WORLD_HALF_W;
