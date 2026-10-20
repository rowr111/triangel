# How to tune the sound response

Every setting that affects how Triangel reacts to sound is in one file, `firmware/shared/src/tuning.rs`, with a comment on each. This page helps you find the right one for what you want to change, and test the change.

## 1. Set up to listen

Play music through real speakers. A phone speaker cannot play the low notes, so the bass bands stay quiet and the beat detection, which listens to the bass, behaves nothing like it will in a room.

It's suggested to try a variety of music within the range of what you expect to play around Triangel - super drum-heavy songs may look different than really melodic ones, songs with a lot of vocals, etc..  you can tune the response to your own personal music taste!

You can watch in the [previewer](run-the-previewer.md) or on a real Triangel. To see numbers as well, build the eye with `bringup` and watch its `audio:` and `drop:` lines; see [Diagnostic output](../reference/diagnostic-output.md#every-5-seconds). `bringup` does not work together with `previewer`, so for numbers, watch on a real Triangel. On the ear's side, its console key `n` shows the bands' range and the level it compares against; see [Mic diagnostic console](../../firmware/ear/README.md#mic-diagnostic-console).

## 2. Find the setting

Each group in `tuning.rs` is a `mod`. Find what you would like in this table:

| If you would like | Where | What to try |
|---|---|---|
| Beats to follow only the bass, not hi-hats and other light hits | `beat` | Raise `LOW_MIN`, so a beat needs more bass |
| Fewer beats | `beat` | Raise `FLUX_TRIGGER` |
| More beats | `beat` | Lower `FLUX_TRIGGER` or `LOW_MIN` |
| More space between beats | `beat` | Raise `BEAT_REFRACTORY_MS`, or lower `FLUX_RELEASE` |
| More or fewer raindrops in Raindrop | `raindrop` | `TRIGGER`, `RELEASE` and `REFRACTORY_MS` work like the `beat` ones; Raindrop has its own trigger |
| Drops caught more readily | `drop_detect` | Lower `BREAKDOWN_DB` or `BREAKDOWN_MS`, or raise `DROP_DB` |
| Drops caught only when they are big | `drop_detect` | Raise `BREAKDOWN_DB` or `BREAKDOWN_MS` |
| Auto to switch to the sound patterns at a different loudness | `level` | Move `ACTIVITY_LOUD_DBFS` toward the `dBFS` the eye reports with music playing |
| Auto to switch sooner or later | `level` | `ACTIVITY_ARM_MS` and `ACTIVITY_RELEASE_MS` |
| More or less contrast between loud and quiet bands | `ear` | `BAND_VISIBLE_RANGE_DB` sets how far below the loudest band counts as dark; `POWER_LAW` sets the contrast |
| The high notes to stand out more | `ear` | Raise `TILT_DB_PER_OCTAVE` |
| The bands evened out, so quieter ones show more | `ear` | Raise `PER_BAND_MIX`, which scales each band to its own range |
| The bands to move snappier or smoother | `ear` | `BAND_ATTACK` and `BAND_DECAY` |
| A pattern brighter or dimmer, or its flashes longer or shorter | The pattern's own group | `spectrum`, `raindrop`, `firework`, `tiles` and `spiderweb` each have their brightness, fade and size settings |

`BASS_BANDS`, at the top, sets which bands count as the bass for the ear's bass level, the beat detection and Raindrop.

## 3. Change one value at a time

Change one value, by a modest step, so you know what made the difference.

Then flash the right board:

- everything except the `ear` group and `BASS_BANDS`: the eye
- the `ear` group: the ear
- `BASS_BANDS`: both

See [How to build the firmware and flash the boards](build-and-flash.md).

## 4. Listen again

Play the same music and compare. Then check the change did not break the other cases:

- the other kinds of music
- a quiet room: sound patterns should still look like something in silence
- a sudden loud sound, like a clap or a door, which should not set off a run of beats

If a pattern needs a new setting of its own, add it to that pattern's group in `tuning.rs`, not to the pattern's file.
