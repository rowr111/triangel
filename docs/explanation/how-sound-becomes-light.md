# How sound becomes light

Sound travels from the microphone, through the ear and the eye, to a pattern. This page follows it, and explains some of the choices along the way.

## The microphone

The microphone is a digital ICS43434 on the controller board. It sends the ear 48,000 measurements a second, each 24 bits. The ear keeps the top 16 bits: they cover 96 dB, from below the microphone's own hiss to louder than any room, so the bottom 8 would only record noise.

## Into frames

The ear averages each pair of measurements, halving the rate to 24,000 a second. That still covers sound up to 12 kHz, the top of what matters for music, for half the work.

It then works out one frame from each 768 measurements, 32 ms of sound. Sound that arrives while it works is skipped rather than let the ear fall behind, so a frame goes out about every 36 ms.

| Setting | Value |
|---|---|
| Microphone rate | 48 kHz, 24-bit, mono, top 16 bits used |
| Rate after halving | 24 kHz, so sound up to 12 kHz |
| Frame | 768 measurements, 32 ms of sound, one about every 36 ms |

These follow from three constants at the top of `firmware/ear/src/audio.rs`.

## 24 bands

The ear splits each frame into 24 bands from 40 Hz to 12 kHz, each about a third of an octave above the last. Every band covers the same musical interval, so the bass gets as many bands per octave as the treble.

It works like a hardware spectrum analyzer: one filter per band passes only that band's frequencies, and the ear measures what comes out. The usual way to split sound into frequencies, an FFT, spaces its frequencies evenly, which would leave the whole bass range with only about two; the filters' spacing better matches how we perceive music.

## Two kinds of loudness

Each frame measures the sound two ways, on purpose:

- The **bands** are compared with the music of the last few seconds. They show which bands are loud compared with the others and never go dark, so a quiet song and a loud one both fill the range.
- The overall **level** and the **bass** level are absolute. They do go quiet in a quiet room, which is how the eye tells music from silence.

The frame also carries the level compared with recent music, and how much the whole spectrum just rose, for finding beats. The settings are in the `ear` group of `firmware/shared/src/tuning.rs`; see [How to tune the sound response](../how-to/tune-the-sound-response.md). 

For the byte layout of the frame sent from the ear to the eye, see [The ear-to-eye link](../reference/ear-to-eye-link.md).

## What the eye works out from the sound frame

From the ear's frames, the eye works out what the patterns use. The code is in [`firmware/eye/src/audio.rs`](../../firmware/eye/src/audio.rs), and the settings in [`firmware/shared/src/tuning.rs`](../../firmware/shared/src/tuning.rs), in the group named below.

### Beats

In `BeatDetector`, with settings in `beat`.

Each frame says how much the whole spectrum just rose, and the eye keeps a slow average of that rise. A beat fires when a frame's rise is well above the average, 1.9 times by default, with enough bass to keep hi-hats from counting. It cannot fire again until the rise falls back and a short gap passes, so one drum hit gives one beat. How far over the average it went sets the beat's strength.

### Onsets

In `Onsets`, with settings in `onset`.

For each band, the eye keeps a fast average that follows the band closely and a slow one that lags behind. The gap between them, from 0 to 1, is the band's onset: large just after a new note or drum in that band, and back to zero as the slow average catches up.

### Drops

In `DropDetector`, with settings in `drop_detect`.

The eye learns the bass's normal level while the bass is present. The bass staying well below normal for a few seconds is a breakdown, and its coming back is the drop. If it never comes back, the eye eventually takes the quiet as the new normal.

### Auto switching

In `ActivityGate`, with settings in `level`.

Each frame is either loud enough to count as music or not. A timer fills while it is loud and empties while it is quiet. Auto switches to the sound patterns only when the timer is full, after 30 seconds of music by default, and back only when it is empty, so music that is only sometimes loud enough does not make it flip back and forth.

### When the ear stops sending

After 200 ms with no frames, the eye fades what the patterns see to quiet, rather than freezing on the last sound.

## Into a pattern

Each time the eye draws, the pattern gets all of this as one `Audio` value: the band levels, the onsets, the loudness, and whether a beat or a drop happened. See [Sound](../reference/pattern-toolkit.md#sound) in the Pattern toolkit for each field, and [Your first sound pattern](../tutorials/first-sound-pattern.md) to try it.
