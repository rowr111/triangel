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

The ear sends one frame per 32 ms of audio (~31 fps) to the eye over a single UART wire. The frame carries 24 band levels, the overall loudness (absolute and relative to recent music), how sharply the spectrum just rose, and the bass level. The eye turns those into beats, drops, per-band onsets and the Auto sound-mode decision.

The baud rate and frame layout are defined once, in the [`shared/`](shared/) crate that both chips build against; see its README for the byte layout.

## Regenerating the LED map

If the PCB geometry or board gap changes:

```powershell
# 1. In "triangel previewer/": regenerate led_map.js
node generate_map.js

# 2. Copy the output into firmware/eye/src/led/map.rs

# 3. In firmware/eye/: regenerate src/led/geom.rs from map.rs
python tools/gen_geom.py
```
