# triangel

A ceiling light fixture in the shape of a ~50cm equilateral triangle, designed to sit in the corner of a room. Built from 25 triangle PCBs (600 WS2812B LEDs total, 24 per board) and driven by custom firmware that runs ambient and sound-reactive lighting patterns.

The installation runs two pattern setlists - ambient and sound-reactive - with a d-pad, 3-position mode switch, and IR remote for control.

## System overview

Two Baochip-1x chips run under [Xous OS](https://betrusted.io/xous-book/):

- **eye** - drives the 600 WS2812 LEDs as two chains in parallel, one BIO co-processor core each, at 30 fps; manages pattern setlists and handles all user input
- **ear** - captures audio from an ICS43434 MEMS mic, measures it in 24 frequency bands, and streams the result to the eye over UART at about 28 fps. The eye uses the band levels to drive sound-reactive patterns.

## Repository structure

| Directory | Contents |
|---|---|
| [`firmware/`](firmware/README.md) | Rust firmware for both chips and the code they share |
| [`firmware/eye/`](firmware/eye/README.md) | Eye chip firmware - LED output, patterns, input handling |
| [`firmware/ear/`](firmware/ear/README.md) | Ear chip firmware - audio capture, filterbank, UART output |
| [`firmware/shared/`](firmware/shared/README.md) | Shared crate - the ear-to-eye frame format and the sound-reactive tuning |
| [`triangel previewer/`](triangel%20previewer/README.md) | Browser-based LED simulator and desktop audio tools |
| `hardware/` | KiCad PCB design files |
| [`graphics/`](graphics/README.md) | Artwork, assembly sticker sheets, and the scripts that generate them |

## Building the kernels (UART2 freed)

The ear-to-eye link uses UART2 on both chips, but a stock Xous kernel gives UART2 to its
log console. Building with the `gdb-stub` feature leaves it free. CI kernels don't have
it, so both chips use a locally built kernel in manual kernel mode.

From a xous-core checkout:

1. `cargo xtask dabao --feature gdb-stub`
2. Copy `loader.uf2` and `xous.uf2` from `target/riscv32imac-unknown-xous-elf/release/`
   into `firmware/xous_build/`. Both chips flash from that one folder.

The eye also needs a fix to the I2C driver (`libs/bao1x-hal/src/udma/i2c.rs`) that is not
in upstream xous-core, so build from a checkout that has that fix.

## Regenerating the I2S mic driver for different pins

The mic's I2S driver runs as a small precompiled BIO program
(`firmware/ear/src/i2s_bio.rs`, generated from `bio-sim/sw/i2s/main.c`). The mic
pins are compiled into it, so changing the mic wiring means regenerating it - a
firmware constant won't do it. Current pins (these match the schematic and
`firmware/ear/src/pins.rs`): BCLK=PB1, SD=PB2, WS=PB3.

To regenerate:

1. One-time: `pip install ziglang`
2. In `bio-sim/sw/i2s/main.c`, set the pin defines (the number is the PBx pin):

       #define WS_PIN  3
       #define SCK_PIN 1
       #define SD_PIN  2

3. From `bio-sim/sw`, run: `python3 -m ziglang build -Dmodule=i2s -Demit-listing=false`
   (regenerates `bio-sim/sw/i2s/i2s.rs`).
4. Copy it in: `cp bio-sim/sw/i2s/i2s.rs firmware/ear/src/i2s_bio.rs`
5. Rebuild the ear firmware.
