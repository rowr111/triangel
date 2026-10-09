# triangel-shared

Code both chips build against, so the **ear** (audio) and the **eye** (LEDs) cannot disagree. No dependencies and no OS requirements.

## `frame` - the ear -> eye frame

The ear sends one `BandFrame` for each 32 ms of audio it captures, about 28 per second, over UART. Baud rate, band count and frame length are constants in [`src/frame.rs`](src/frame.rs) (`EAR_UART_BAUD`, `BAND_COUNT`, `FRAME_LEN`).

### Wire format (58 bytes, little-endian)

| Offset | Size | Content |
|---|---|---|
| 0x00 | 1 | Sync byte `0xAA` |
| 0x01-0x30 | 48 | `bands`: 24 x u16, band 0 (lowest) first |
| 0x31-0x32 | 2 | `level`: overall loudness, absolute dBFS |
| 0x33-0x34 | 2 | `level_norm`: loudness relative to the recent loudest and quietest |
| 0x35-0x36 | 2 | `flux`: how much the whole spectrum rose this frame |
| 0x37-0x38 | 2 | `bass`: level of the lowest bands, absolute dBFS |
| 0x39 | 1 | XOR checksum of bytes 0x01-0x38 |

`bands` are normalized against recent music, so they show spectral shape but never go dark in a quiet room. `level` and `bass` are absolute, so they do. dB SPL is roughly dBFS + 120 with this microphone.

Values go on and off the wire through `level_to_wire` / `level_from_wire` (dBFS) and `norm_to_wire` / `norm_from_wire` (0.0-1.0).

### Usage

```rust
use triangel_shared::frame::{BandFrame, FRAME_LEN};

// ear
let mut buf = [0u8; FRAME_LEN];
frame.encode(&mut buf);

// eye - None on a bad sync byte or checksum
if let Some(frame) = BandFrame::decode(&buf) { /* ... */ }
```

The eye receives a byte stream, not whole frames. `FrameAssembler::feed` takes one byte at a time, finds the sync byte, and returns a `BandFrame` each time one completes.

## `tuning` - sound-reactive tuning

Every knob for beat, onset and drop detection, the Auto sound-mode switch, each reactive pattern, and the ear's normalization, in one file: [`src/tuning.rs`](src/tuning.rs). The `ear` section needs the ear reflashed and `BASS_BANDS` needs both chips; everything else only the eye.
