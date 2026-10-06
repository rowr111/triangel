# triangel - controller firmware

The triangel fixture is driven by two Baochip-1x chips running [Xous OS](https://betrusted.io/xous-book/) (DABAO dev boards used during development):

| Chip | Role |
|---|---|
| **eye** | Drives 600 WS2812 LEDs, handles controls, renders patterns |
| **ear** | Captures audio, runs a 24-band filterbank, streams the result to eye over UART |

See [`eye/`](eye/) and [`ear/`](ear/) for crate-level documentation.

## Physical fixture

The fixture mounts in the corner of a room - an equilateral triangle cutting off the corner tip. It is made of 25 triangle PCBs (24 LEDs each) arranged in a larger triangle:

```
->  1   2   3   4   5   6   7   8   9
<- 16  15  14  13  12  11  10
->     17  18  19  20  21
<-         24  23  22
                25
```

Boards are numbered left-to-right, top-to-bottom by position. The data path snakes for shortest inter-board wire lengths (~52 mm per jump) and is split into two chains, driven in parallel:

| Chain | Eye pin | Enters at | Boards, in order | LEDs |
|---|---|---|---|---|
| 1 | PB4 | board 1 | 1-9, 16, 15, 14 | 288 |
| 2 | PB5 | board 13 | 13, 12, 11, 10, 17-21, 24, 23, 22, 25 | 312 |

## Eye <-> ear communication

The ear sends one frame for each 32 ms of audio it captures, about 28 per second, to the eye over a single UART wire. The frame carries 24 band levels, the overall loudness (absolute and relative to recent music), how sharply the spectrum just rose, and the bass level. The eye turns those into beats, drops, per-band onsets and the Auto sound-mode decision.

The baud rate and frame layout are defined once, in the [`shared/`](shared/) crate that both chips build against; see its README for the byte layout.

## Building

Rust stays at 1.90.0: the Xous library that `install-toolkit` downloads only works with the version it was built for. One-time setup:

```powershell
rustup toolchain install 1.90.0
rustup override set 1.90.0   # in the xous-core checkout and in this repo's root
cargo xtask install-toolkit  # in the xous-core checkout
```

Install the pre-commit hook, which runs the checks in `.pre-commit-config.yaml` before each commit (needs Python):

```powershell
pip install pre-commit
pre-commit install   # in this repo's root
```

Build and flash with the Baochip VS Code extension: out-of-tree, kernel mode manual, kernel files from each chip's `xous_build` folder. Never use ci-sync; it replaces the kernels built in the root README's "Building the kernels".

## Regenerating the LED map

If the PCB geometry or board gap changes:

```powershell
# 1. In "triangel previewer/": regenerate led_map.js
node generate_map.js

# 2. Copy the output into firmware/eye/src/led/map.rs

# 3. In firmware/eye/: regenerate src/led/geom.rs from map.rs
python tools/gen_geom.py
```
