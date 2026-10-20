# Your first sound pattern

In this tutorial you will write a pattern that reacts to music: it gets brighter when the music is louder, flashes on the beat, and shows the low and high notes across the triangel.

## Before you start

Do [Your first pattern](first-pattern.md) first.

This time you need both dabao boards and the controller board. The ear does the listening, and the controller board holds the microphone and connects the ear to the eye:

- a controller board with the eye, the ear and the microphone in it
- two USB cables, one for each dabao
- music playing near the microphone, from real speakers

> [!NOTE]
> A phone speaker cannot play the low notes, so the bass bands stay quiet and the sound patterns do not react the way they will in a room. Use real speakers of some kind, for this tutorial and whenever you work on sound patterns.

The triangle tiles and the 12 V supply are not required: the dabaos run off their USB cables, and the previewer shows the patterns.

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. For this tutorial, leave the triangle tiles unplugged.

## 1. Flash the ear

The ear is the board that listens. It measures the sound in 24 frequency bands, from low notes to high, and sends them to the eye.

Plug the ear into your computer with a USB cable, and put it in bootloader mode by holding **PROG** while you press **RESET**.

Open the `firmware/ear` folder in VS Code and click **Build - Flash - Boot** in the Baochip sidebar. The ear has no build options.

> **Or, instead, from the command line:** from `firmware/ear`, run
>
> ```
> cargo build --release
> xous-app-uf2 --elf target/riscv32imac-unknown-xous-elf/release/ear
> ```
>
> Copy `apps.uf2` onto the drive, then press **PROG**. If this dabao has never had triangel's kernel, copy `firmware/xous_build/loader.uf2` and `firmware/xous_build/xous.uf2` as well.

You only need to flash the ear once. Everything you change in this tutorial is in the eye.

## 2. Flash the eye and check the sound

Plug the eye into your computer, hold **PROG** while you press **RESET**, and flash it with the `previewer` option as in [Your first run](first-run.md). Start the bridge and open the previewer page.

Set the switch on the page to **On**. triangel changes to its sound patterns. The sound switch on the input board works too: whichever one you change last wins. Play some music: the first sound pattern, Tiles, lights whole triangle tiles on the beat.

If nothing reacts, check that the ear was flashed and that both boards are seated in the controller board.

## 3. Create the pattern

Sound patterns live next to the ambient ones, in their own folder. Create `firmware/eye/src/patterns/reactive/my_sound.rs`:

```rust
use crate::audio::Audio;
use crate::patterns::{Frame, Pattern, hsv};

pub struct MySound;

impl MySound {
    pub fn new() -> Self {
        MySound
    }
}

impl Pattern for MySound {
    fn render(&mut self, _t_ms: u32, audio: &Audio, out: &mut Frame) {
        // Brighter when louder, but never dark, so a quiet room still shows something.
        let brightness = 0.2 + 0.8 * audio.level_norm;
        out.fill(hsv(200.0, 1.0, brightness));
    }
}
```

This time `render` uses `audio`. `audio.level_norm` is how loud the music is compared with the music of the last little while, from 0 to 1. Because it is compared with recent music, quiet music and loud music both use the whole range.

The pattern never goes fully dark. A sound pattern can be on screen in a quiet room, and it should still look like something there.

`new()` does nothing yet. The next step needs it.

## 4. Add it to the sound setlist

Like in the first pattern, two small edits. In `firmware/eye/src/patterns/reactive/mod.rs`:

```rust
pub mod my_sound;
```

In `firmware/eye/src/setlist.rs`, add `my_sound::MySound` to the list of reactive patterns it uses:

```rust
use crate::patterns::reactive::{
    firework::Firework, my_sound::MySound, raindrop::Raindrop, spectrum::Spectrum, spiderweb::Spiderweb, tiles::Tiles,
};
```

Then put it first in `reactive_patterns()`:

```rust
fn reactive_patterns() -> Vec<Box<dyn Pattern>> {
    vec![
        Box::new(MySound::new()),
        Box::new(Tiles::new()),
        ...
```

Hold **PROG** while you press **RESET** on the eye, then click **Build - Flash - Boot**. Make sure the sound switch is on **On**, on the page or the input board. Play music, and the whole triangel pulses blue with the loudness.

![The previewer with the triangel pulsing blue](../images/first-sound-pattern-loudness.png)

## 5. Flash on the beat

`audio.beat` is true for one frame when a beat comes. One frame is too short to see, so the pattern needs to remember when the beat came and fade a flash out over the next few hundred milliseconds. That means `MySound` needs to hold something. Replace the file with:

```rust
use crate::audio::Audio;
use crate::patterns::{Frame, Pattern, hsv};

// How long a beat's flash takes to fade.
const FLASH_MS: u32 = 300;

pub struct MySound {
    // When the last beat came.
    beat_ms: Option<u32>,
}

impl MySound {
    pub fn new() -> Self {
        MySound { beat_ms: None }
    }
}

impl Pattern for MySound {
    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.beat {
            self.beat_ms = Some(t_ms);
        }
        // 1 right on the beat, fading to 0 over FLASH_MS.
        let flash = match self.beat_ms {
            Some(at) if t_ms.wrapping_sub(at) < FLASH_MS => 1.0 - t_ms.wrapping_sub(at) as f32 / FLASH_MS as f32,
            _ => 0.0,
        };

        let brightness = 0.2 + 0.8 * audio.level_norm;
        // On a beat, wash toward full-brightness white.
        out.fill(hsv(200.0, 1.0 - flash, brightness.max(flash)));
    }
}
```

`wrapping_sub` gives how long ago the beat was. Use it rather than `-` for times: `t_ms` starts again from 0 after about 49 days, and `wrapping_sub` still gives the right answer when it does.

Build, flash and play music. The triangel now flashes white on each beat.

## 6. Show the notes across the triangel

`audio.bands` holds the 24 bands, lowest notes first, each from 0 to 1. Give each LED the band for where it sits, low notes on the left and high notes on the right. Change the `use` lines at the top to:

```rust
use crate::audio::Audio;
use crate::led::map::LED_MAP;
use crate::led::world::{WORLD_LEFT, WORLD_RIGHT};
use crate::patterns::{Frame, Pattern, hsv};
use triangel_shared::frame::BAND_COUNT;
```

and replace everything in `render` after the `flash` calculation, from `let brightness` to the end, with:

```rust
        for (i, led) in LED_MAP.iter().enumerate() {
            // Low notes on the left, high notes on the right.
            let across = (led.wx - WORLD_LEFT) / (WORLD_RIGHT - WORLD_LEFT);
            let band = ((across * BAND_COUNT as f32) as usize).min(BAND_COUNT - 1);
            // Blue for the low bands through to purple for the high ones.
            let hue = 200.0 + 80.0 * across;
            let brightness = 0.2 + 0.8 * audio.bands[band];
            out[i] = hsv(hue, 1.0 - flash, brightness.max(flash));
        }
```

Build, flash and play music. The bass moves the left side, the high notes move the right side, and the whole triangel still flashes on the beat.

![The previewer showing the bands across the triangel](../images/first-sound-pattern-bands.png)

## 7. Make it your own

Try a few changes:

- **Flash length:** change `FLASH_MS` to `120` for a sharp flick, or `800` for a long glow.
- **Jumpier:** use `audio.rise[band]` in place of `audio.bands[band]`. `rise` is how far each band has just jumped above its own recent level, so it lights up on new notes and drums instead of following the overall shape.
- **Your own colors:** change the hue range. Keep it to a few related colors; they read better on the triangel than a full rainbow.

## What you did

You flashed the ear, wrote a pattern that reads the sound, and used the three main things the eye hears: how loud it is, when the beat comes, and how loud each band is.

When you are done, take `Box::new(MySound::new())` out of the front of `reactive_patterns()`, or move it to where you want it.

Next:

- [Sound](../reference/pattern-toolkit.md#sound), in the Pattern toolkit, for everything `audio` offers and where sound tuning values go
- [How to add a pattern](../how-to/add-a-pattern.md), for your next one
