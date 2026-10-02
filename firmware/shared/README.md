# triangel-shared

Code both chips build against, so the **ear** (audio) and the **eye** (LEDs) cannot disagree. No dependencies and no OS requirements.

## `mel` - the ear -> eye frame

The ear sends one `MelFrame` per 32 ms of audio (~31 fps) over UART. Baud rate, band count and frame length are constants in [`src/mel.rs`](src/mel.rs) (`EAR_UART_BAUD`, `MEL_BANDS`, `FRAME_LEN`).

### Wire format (59 bytes, little-endian)

| Offset | Size | Content |
|---|---|---|
| 0x00 | 1 | Sync byte `0xAA` |
| 0x01-0x30 | 48 | `bands`: 24 x u16, band 0 (lowest) first |
| 0x31-0x32 | 2 | `level`: overall loudness, absolute dBFS |
| 0x33-0x34 | 2 | `level_norm`: loudness relative to the recent loudest and quietest |
| 0x35-0x36 | 2 | `flux`: how much the whole spectrum rose this frame |
| 0x37-0x38 | 2 | `bass`: level of the lowest bands, absolute dBFS |
| 0x39 | 1 | Activity flag (not used by the eye) |
| 0x3A | 1 | XOR checksum of bytes 0x01-0x39 |

`bands` are normalized against recent music, so they show spectral shape but never go dark in a quiet room. `level` and `bass` are absolute, so they do. dB SPL is roughly dBFS + 120 with this microphone.

Values go on and off the wire through `level_to_wire` / `level_from_wire` (dBFS) and `norm_to_wire` / `norm_from_wire` (0.0-1.0).

### Usage

```rust
use triangel_shared::mel::{MelFrame, FRAME_LEN};

// ear
let mut buf = [0u8; FRAME_LEN];
frame.encode(&mut buf);

// eye - None on a bad sync byte or checksum
if let Some(frame) = MelFrame::decode(&buf) { /* ... */ }
```

## `tuning` - sound-reactive tuning

Every knob for beat, onset and drop detection, the Auto sound-mode switch, each reactive pattern, and the ear's normalization, in one file: [`src/tuning.rs`](src/tuning.rs). The `ear` section needs the ear reflashed; everything else only the eye.
