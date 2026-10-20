# The ear-to-eye link

The ear sends its sound measurements to the eye over one wire, from the ear's PB14 to the eye's PB13, as a stream of fixed-size frames. Both boards build against the same code for it, in `firmware/shared/src/frame.rs`, so a change there means flashing both. The speed, band count and frame length are constants in that file: `EAR_UART_BAUD`, `BAND_COUNT` and `FRAME_LEN`.

## Frame layout

58 bytes, little-endian:

| Offset | Size | Content |
|---|---|---|
| 0x00 | 1 | Sync byte `0xAA` |
| 0x01-0x30 | 48 | `bands`: 24 x u16, band 0 (lowest) first |
| 0x31-0x32 | 2 | `level`: overall loudness, absolute dBFS |
| 0x33-0x34 | 2 | `level_norm`: loudness relative to the recent loudest and quietest |
| 0x35-0x36 | 2 | `flux`: how much the whole spectrum rose this frame |
| 0x37-0x38 | 2 | `bass`: level of the lowest bands, absolute dBFS |
| 0x39 | 1 | XOR checksum of bytes 0x01-0x38 |

`bands` are normalized against recent music, so they show the shape of the sound but never go dark in a quiet room. `level` and `bass` are absolute, so they do. dB SPL is roughly dBFS + 120 with this microphone.

Values go on and off the wire through `level_to_wire` / `level_from_wire` (dBFS) and `norm_to_wire` / `norm_from_wire` (0.0-1.0).

## Using it in code

```rust
use triangel_shared::frame::{BandFrame, FRAME_LEN};

// ear
let mut buf = [0u8; FRAME_LEN];
frame.encode(&mut buf);

// eye - None on a bad sync byte or checksum
if let Some(frame) = BandFrame::decode(&buf) { /* ... */ }
```

The eye receives a stream of bytes, not whole frames. `FrameAssembler::feed` takes one byte at a time, finds the sync byte, and returns `Fed::Frame` when a frame completes, `Fed::Bad` when one fails its checksum, and `Fed::Pending` otherwise.

To see whether frames are arriving, see the eye's `audio:` line in [Diagnostic output](diagnostic-output.md#every-5-seconds).
