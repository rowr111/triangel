# triangel-shared

Shared types and wire protocol definitions for the triangel two-chip lighting system. Both the **eye** chip (LED controller) and the **ear** chip (audio processor) depend on this crate so the encoder and decoder are guaranteed to match.

## Contents

### `mel` — ear→eye mel frame protocol

The ear chip streams 24 mel frequency band values to the eye chip over UART at ~30 fps.

#### Wire frame format (51 bytes)

| Offset | Size | Content |
|--------|------|---------|
| 0x00 | 1 byte | Sync byte: `0xAA` |
| 0x01-0x30 | 48 bytes | 24 x u16 little-endian mel bands (band 0 first) |
| 0x31 | 1 byte | Activity flag (0 = quiet, 1 = music active) |
| 0x32 | 1 byte | XOR checksum of bytes 0x01-0x31 |

Band values are `u16` scaled 0-65535. The eye chip divides by 65535.0 to get 0.0-1.0 floats before applying attack/decay smoothing. The activity flag is set by the ear chip based on sustained absolute energy exceeding a calibrated threshold.

#### Usage

**Ear chip (encode):**
```rust
use triangel_shared::mel::{MelFrame, FRAME_LEN};

let frame = MelFrame { bands: computed_bands, activity: is_loud };
let mut buf = [0u8; FRAME_LEN];
frame.encode(&mut buf);
uart.write_all(&buf).ok();
```

**Eye chip (decode):**
```rust
use triangel_shared::mel::{MelFrame, FRAME_LEN};

let mut buf = [0u8; FRAME_LEN];
uart.read_exact(&mut buf).ok();
if let Some(frame) = MelFrame::decode(&buf) {
    // frame.bands[0..24] - mel band values
    // frame.activity     - true when sustained loudness exceeds ear's threshold
}
```

## Adding as a dependency

```toml
triangel-shared = { path = "../shared" }
```

This crate has no external dependencies and no OS requirements — safe to use in both chips.
