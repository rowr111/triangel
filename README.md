# triangel

A ceiling light fixture in the shape of a ~50cm equilateral triangle, designed to sit in the corner of a room. Built from 25 triangle PCBs (600 WS2812B LEDs total, 24 per board) and driven by custom firmware that runs ambient and sound-reactive lighting patterns.

The installation runs two pattern setlists — ambient and sound-reactive — with a d-pad, 3-position mode switch, and IR remote for control.

## System overview

Two Baochip-1x chips run under [Xous OS](https://betrusted.io/xous-book/):

- **eye** — drives the 600-LED WS2812 chain via a BIO co-processor core at 30 fps, manages pattern setlists, and handles all user input
- **ear** — captures audio from an ICS43434 MEMS mic, computes a 24-band mel filterbank, and streams the result to the eye over UART at ~30 fps. The eye uses the mel band data to drive sound-reactive patterns.

## Repository structure

| Directory | Contents |
|---|---|
| [`firmware/`](firmware/README.md) | Rust firmware for both chips and shared wire protocol types |
| [`firmware/eye/`](firmware/eye/README.md) | Eye chip firmware — LED output, patterns, input handling |
| [`firmware/ear/`](firmware/ear/README.md) | Ear chip firmware — audio capture, mel filterbank, UART output |
| [`firmware/shared/`](firmware/shared/README.md) | Shared crate — mel frame wire protocol used by both chips |
| [`triangel previewer/`](triangel%20previewer/README.md) | Browser-based LED simulator and desktop audio tools |
| `hardware/` | KiCad PCB design files |
| `graphics/` | Artwork and graphic assets |

## Building the ear kernel (UART2 freed)

The ear sends audio-level data to the eye over hardware UART2, but a stock Xous
build gives UART2 to the log server's debug console. Building with the `gdb-stub`
feature compiles that out, freeing UART2 for the ear->eye link. The extension's
ci-sync kernels don't have this, so the ear uses a locally built kernel in manual
kernel mode.

From the xous-core checkout (on `dev`):

1. Build the dabao kernel with UART2 freed:

       cargo xtask dabao --feature gdb-stub

2. Copy the resulting `loader.uf2` and `xous.uf2` into `firmware/ear/xous_build/`.

3. Build and flash the ear firmware with the VSCode extension (manual kernel mode
   uses the kernel you just copied).

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
