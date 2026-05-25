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
