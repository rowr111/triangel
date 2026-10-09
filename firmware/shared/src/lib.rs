pub mod frame;
pub mod tuning;

/// Move `cur` one step toward `target`: by the fraction `up` when rising, `down` when falling.
#[inline]
pub fn follow(cur: f32, target: f32, up: f32, down: f32) -> f32 {
    let rate = if target > cur { up } else { down };
    cur + (target - cur) * rate
}
