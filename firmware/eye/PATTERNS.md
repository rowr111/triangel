# Writing patterns

Patterns implement the `Pattern` trait in `src/patterns/`. Each gets called once per frame with world-position metadata for every LED, the current time, and the current sound level.

## Minimal example

```rust
// src/patterns/your_pattern.rs
use super::{Frame, Pattern};
use crate::led::map::Led;

pub struct YourPattern {
    pub some_param: f32,
}

impl Pattern for YourPattern {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        for (i, led) in leds.iter().enumerate() {
            // led.wx, led.wy - position in mm
            // t_ms - monotonic time
            out[i] = [r, g, b];
        }
    }
}
```

Then add it to the setlists in `src/setlist.rs`:

```rust
// in ambient_patterns() or reactive_patterns():
Box::new(YourPattern { some_param: 1.0 }),
```

And add `pub mod your_pattern;` to `src/patterns/mod.rs`.

## World coordinates

```
origin: top-left of fixture
x: increases rightward, 0-517 mm
y: increases downward,  0-436 mm

WORLD_CX  = 258 mm  - horizontal centre
WORLD_TOP =   6 mm  - topmost LED
WORLD_BOT = 436 mm  - apex (board 25, room corner)
WORLD_H   = 430 mm  - total height
```

These constants are exported from `crate::led::map` and available to all patterns.

## Utilities

```rust
// src/patterns/mod.rs
hsv(h: f32, s: f32, v: f32) -> [u8; 3]   // h: 0-360, s/v: 0-1
lerp(a: f32, b: f32, t: f32) -> f32
clamp(x: f32, lo: f32, hi: f32) -> f32
```

## Sound-reactive patterns

For simple sound reactivity, use the `sound_level` argument (0.0-1.0, pre-smoothed with attack 0.25 / decay 0.02).

For custom smoothing, hold an `Envelope` as a field and feed it raw mel band data:

```rust
use super::{Envelope, Frame, Pattern};
use crate::audio::AudioReceiver;

pub struct MyReactivePattern {
    envelope: Envelope,
}

impl MyReactivePattern {
    pub fn new() -> Self {
        Self { envelope: Envelope::new(0.5, 0.05) } // faster attack, slower decay
    }
}

impl Pattern for MyReactivePattern {
    fn render(&mut self, leds: &[Led], t_ms: u32, _sound_level: f32, out: &mut Frame) {
        // drive level from a specific mel band instead of the global scalar
        // (mel data not yet wired - placeholder uses sound_level for now)
        let level = self.envelope.update(_sound_level);
        // ...
    }
}
```

Once the ear chip is wired, patterns can call `audio.current_mel()` directly to get the 24-band array and pick the frequency bands most relevant to their visual effect.
