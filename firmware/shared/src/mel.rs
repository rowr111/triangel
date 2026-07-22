/// Baud rate for the ear->eye UART link. Must match on both chips.
pub const EAR_UART_BAUD: u32 = 1_000_000;

/// Sync byte that starts every mel frame on the ear->eye UART wire.
pub const SYNC_BYTE: u8 = 0xAA;

/// Number of mel frequency bands the ear chip computes.
pub const MEL_BANDS: usize = 24;

/// Wire frame length in bytes: 1 sync + MEL_BANDS*2 bands + 2 level + 1 activity + 1 checksum.
pub const FRAME_LEN: usize = 1 + MEL_BANDS * 2 + 2 + 1 + 1; // 53 bytes

// FUTURE (step 2b): the ear will also send less-processed views so patterns can
// choose. Planned additions to MelFrame + the wire format, appended before the
// checksum so decode stays forward-compatible if versioned:
//   - raw_bands: [u16; MEL_BANDS]  -- log energy, lightly smoothed, NOT gain-normalized,
//     so a pattern can show honest relative loudness and go dark when it is actually quiet.
//   - reductions: e.g. bass/mid/treble sums and/or an onset/beat flag, computed on the
//     ear to save the eye from recomputing them across 600 LEDs every frame.
// Not added yet; `bands` (normalized) + `level` + `activity` is the 2a set.

/// One frame of mel band data sent from the ear chip to the eye chip.
///
/// Wire format (53 bytes, little-endian):
///
/// ```text
/// [0x00]        SYNC_BYTE (0xAA)
/// [0x01..0x30]  bands[0..23] as u16 little-endian  (48 bytes)
/// [0x31..0x32]  level as u16 little-endian          (2 bytes)
/// [0x33]        activity flag (0 = quiet, 1 = music active)
/// [0x34]        XOR checksum of bytes [0x01..0x33]
/// ```
///
/// `bands` are AGC-normalized + smoothed and scaled so 0 = silence and 65535 = full
/// scale (the eye divides by 65535.0). `level` is one overall loudness value on the
/// same scale, for simple level-reactive patterns. The eye chip divides by 65535.0.
///
/// The activity flag is set by the ear chip based on sustained absolute energy
/// exceeding a calibrated threshold - the eye uses it for Auto sound mode without
/// needing to reason about absolute levels itself.
pub struct MelFrame {
    pub bands:    [u16; MEL_BANDS],
    pub level:    u16,
    pub activity: bool,
}

impl MelFrame {
    /// Build a frame that carries only an overall level + activity, with the bands
    /// zeroed. Used before the mel FFT is enabled (or anywhere a level is all that's
    /// available), so the wire format stays the same 53-byte frame either way.
    pub fn level_only(level: u16, activity: bool) -> Self {
        MelFrame { bands: [0; MEL_BANDS], level, activity }
    }

    /// Serialise into a 53-byte wire buffer.
    pub fn encode(&self, buf: &mut [u8; FRAME_LEN]) {
        buf[0] = SYNC_BYTE;
        for (i, &band) in self.bands.iter().enumerate() {
            let off = 1 + i * 2;
            buf[off]     = (band & 0xFF) as u8;
            buf[off + 1] = (band >> 8)   as u8;
        }
        let lvl_off = 1 + MEL_BANDS * 2;
        buf[lvl_off]     = (self.level & 0xFF) as u8;
        buf[lvl_off + 1] = (self.level >> 8)   as u8;
        buf[FRAME_LEN - 2] = self.activity as u8;
        let checksum = buf[1..FRAME_LEN - 1].iter().fold(0u8, |acc, &b| acc ^ b);
        buf[FRAME_LEN - 1] = checksum;
    }

    /// Parse a 53-byte wire buffer. Returns `None` if sync or checksum is wrong.
    pub fn decode(buf: &[u8; FRAME_LEN]) -> Option<Self> {
        if buf[0] != SYNC_BYTE {
            return None;
        }
        let expected = buf[1..FRAME_LEN - 1].iter().fold(0u8, |acc, &b| acc ^ b);
        if buf[FRAME_LEN - 1] != expected {
            return None;
        }
        let mut bands = [0u16; MEL_BANDS];
        for (i, band) in bands.iter_mut().enumerate() {
            let off = 1 + i * 2;
            *band = (buf[off] as u16) | ((buf[off + 1] as u16) << 8);
        }
        let lvl_off = 1 + MEL_BANDS * 2;
        let level = (buf[lvl_off] as u16) | ((buf[lvl_off + 1] as u16) << 8);
        let activity = buf[FRAME_LEN - 2] != 0;
        Some(MelFrame { bands, level, activity })
    }
}
