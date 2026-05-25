# triangel - controller firmware

The triangel fixture is driven by two Baochip-1x chips running [Xous OS](https://betrusted.io/xous-book/) (DABAO dev boards used during development):

| Chip | Role |
|---|---|
| **eye** | Drives 600 WS2812 LEDs, handles controls, renders patterns |
| **ear** | Captures audio, computes mel filterbank, streams data to eye over UART |

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

Boards are numbered left-to-right, top-to-bottom by position. The data chain snakes for shortest inter-board wire lengths (~52 mm per jump). Baochip data wire attaches at **board 1** (top-left); chainIdx 0 = board 1, first LED.

## Eye <-> ear communication

The ear chip sends mel data to the eye chip over UART at ~30 fps. The wire frame format (51 bytes) is defined in [`shared/`](shared/) (`triangel-shared` crate):

| Offset | Size | Content |
|---|---|---|
| 0x00 | 1 byte | Sync byte `0xAA` |
| 0x01-0x30 | 48 bytes | 24 x u16 little-endian mel bands |
| 0x31 | 1 byte | Activity flag (0 = quiet, 1 = music active) |
| 0x32 | 1 byte | XOR checksum of bytes 0x01-0x31 |

The ear chip sets the activity flag based on sustained absolute energy exceeding a calibrated threshold - the eye uses this for Auto sound mode. The eye applies its own attack/decay envelope to the band values to produce a smoothed level for patterns; raw per-band values are also available for patterns that want custom smoothing.

## Regenerating the LED map

If the PCB geometry or board gap changes, regenerate `led_map.js` in the previewer then update `map.rs` in eye:

```powershell
# In triangel previewer/
node generate_map.js

# Copy the output into firmware/eye/src/led/map.rs
```
