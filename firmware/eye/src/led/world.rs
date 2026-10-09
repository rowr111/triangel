use super::map::WORLD_CX;

// Half the width of the fixture's top edge, about WORLD_H / sqrt(3).
pub const WORLD_HALF_W: f32 = 248.0;
pub const WORLD_LEFT:   f32 = WORLD_CX - WORLD_HALF_W;
pub const WORLD_RIGHT:  f32 = WORLD_CX + WORLD_HALF_W;
