# Pattern toolkit

What a pattern has to work with: the `Pattern` trait, the Triangel's measurements, the helpers, and the sound. To write your first pattern, see [Your first pattern](../tutorials/first-pattern.md). Paths here are relative to `firmware/eye/`.

## The Pattern trait

A pattern implements `Pattern`, in `src/patterns/mod.rs`:

```rust
pub trait Pattern: Send {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame);
    fn on_enter(&mut self, _t_ms: u32) {}
}
```

- `render` is called once per frame, about 30 times a second. It fills in `out`, one color per LED. `out[i]` is the color of `LED_MAP[i]`.
- `t_ms` is the time in milliseconds. It is a `u32` and wraps after 49.7 days, so compare times with `wrapping_sub`, never `<` or `>=`.
- `audio` is this frame's sound. Ambient patterns ignore it.
- `on_enter` runs before the first frame each time the pattern comes on screen after being off it for more than half a second. A pattern is only rendered while it is on screen or fading out, so time jumps when it comes back. Use `on_enter` to reset whatever should start fresh.

Ambient patterns live in `src/patterns/ambient/`, sound-reactive ones in `src/patterns/reactive/`.

## The Triangel's measurements

```
origin: top-left of the Triangel
x: increases rightward, 0-517 mm
y: increases downward,  0-436 mm

WORLD_CX  = 258.7 mm  - horizontal center
WORLD_TOP =   6 mm    - topmost LED
WORLD_BOT = 436 mm    - apex (triangle tile 25, room corner)
WORLD_H   = 430 mm    - total height
```

These constants, plus `WORLD_CENTROID_X` / `WORLD_CENTROID_Y`, are in `crate::led::map`. The triangle's left and right edges are `WORLD_LEFT` / `WORLD_RIGHT` in `crate::led::world`.

Each entry of `LED_MAP` has the LED's position, `wx` and `wy` in mm, its triangle tile (`board_id`) and its number on that triangle tile (`local_idx`, 1-24). `LED_MAP` is in wiring order, so an LED's index is also its place on the chain.

Fixed values worked out from the map are in `crate::led::geom`:

- per LED: `DIST_C[i]` and `THETA_C[i]`, the distance and angle from the centroid, and `DIST_APEX[i]`
- per triangle tile, by its number: `BOARD_CENTER` and `BOARD_Y_RANGE`
- `BOARD_COUNT`, `LEDS_PER_BOARD`, `DIST_C_MAX`

`crate::led::grid` groups LEDs by position: `for_each_near(x, y, reach, f)` and `for_each_near_seg(a, b, reach, f)` call `f` with the index of each LED near a point or a segment.

## Helpers

In `src/patterns/mod.rs`:

```rust
hsv(h, s, v) -> [u8; 3]     // h: 0-360, s/v: 0-1
lerp(a, b, t) -> f32
Phasor::new(angle)          // .sin, .cos, .sin_plus(b), .sin_minus(b); see How to make a pattern fast enough
mix_rgb(a, b, t)            // lerp per channel, f32 RGB
ramp(&stops, t)             // color along (position, color) stops
smoothstep(t) -> f32        // clamped to 0-1
wrap360(h) -> f32           // hue into 0-360, for inputs within one turn of range
cycle(t_ms, period_ms)      // 0-1 position in a repeating period
cycle_q16(t_ms, period_ms)  // the same as a whole number, x65536
fold_ms(t_ms, rate)         // t_ms folded to one period of a rad/ms rate
slot_life(slot, salt, t_ms, min_ms, max_ms) // (generation, 0-1 age) of a slot that restarts with no stored state
hash2(a, b), knuth_hash(x), tile_hash(led)
Rng::new(seed)              // .f32(), .next_u32(), .range_f32(min, max), .range_u32(min, max), .below(n), .pick(&items)
free_or_oldest(slots, now_ms, is_free, start_ms)  // a free slot in a pool, else the oldest
Shot                        // a flash that starts white, holds, then fades
ShotPair                    // the two latest shots; .add(shot, t_ms), .brightest(t_ms)
phase_phasors()             // see How to make a pattern fast enough
```

Shared building blocks:

- `glints::Glints` / `GlintStyle`: brief white flashes on single LEDs, mostly on the bright ones. Draw the frame, then call `update` over it.
- `ripples::Ripples` / `RippleStyle`: trains of raindrop rings on black. The pattern picks where and when each drop falls (`spot`, `land`); `draw` renders them.

## Sound

`Audio`, in `src/audio.rs`, is this frame's view of the sound:

| Field | Meaning |
|---|---|
| `bands: [f32; 24]` | Band levels 0-1, lowest frequency first. Normalized, so they keep their shape when quiet |
| `rise: [f32; 24]` | How far each band just jumped above its own recent average, 0-1 |
| `level_norm` | Loudness relative to recent music, 0-1 |
| `beat`, `beat_strength` | `beat` is true for the one frame a beat comes; strength is 0-1 |
| `drop` | True for the one frame the bass returns after a breakdown |

A sound-reactive pattern has to look good at every level, silence included.

A sound-reactive pattern's tuning constants go in their own module in `firmware/shared/src/tuning.rs`, alongside the detection thresholds they interact with. See [How to tune the sound response](../how-to/tune-the-sound-response.md).
