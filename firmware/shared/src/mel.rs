/// Baud rate for the ear->eye UART link. Must match on both chips.
pub const EAR_UART_BAUD: u32 = 1_000_000;

/// Sync byte that starts every mel frame on the ear->eye UART wire.
pub const SYNC_BYTE: u8 = 0xAA;

/// Number of mel frequency bands the ear chip computes.
pub const MEL_BANDS: usize = 24;

/// Quietest level the wire carries. LEVEL_DB_FLOOR..0 dBFS maps onto 0..=65535.
pub const LEVEL_DB_FLOOR: f32 = -90.0;

/// Encode dBFS for the wire. Both chips go through this so they cannot disagree.
pub fn level_to_wire(dbfs: f32) -> u16 {
    let t = ((dbfs - LEVEL_DB_FLOOR) / -LEVEL_DB_FLOOR).clamp(0.0, 1.0);
    (t * 65535.0) as u16
}

/// Decode a wire level back to absolute dBFS.
pub fn level_from_wire(level: u16) -> f32 {
    LEVEL_DB_FLOOR + (level as f32 / 65535.0) * -LEVEL_DB_FLOOR
}

/// Encode a 0.0-1.0 normalized level for the wire. Both chips go through this so
/// they cannot disagree.
pub fn norm_to_wire(norm: f32) -> u16 {
    (norm.clamp(0.0, 1.0) * 65535.0) as u16
}

/// Decode a wire normalized level back to 0.0-1.0.
pub fn norm_from_wire(norm: u16) -> f32 {
    norm as f32 / 65535.0
}

/// Wire frame length in bytes: 1 sync + MEL_BANDS*2 bands + 2 level + 2 level_norm
/// + 1 activity + 1 checksum.
pub const FRAME_LEN: usize = 1 + MEL_BANDS * 2 + 2 + 2 + 1 + 1; // 55 bytes

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
/// [0x33..0x34]  level_norm as u16 little-endian     (2 bytes)
/// [0x35]        activity flag (0 = quiet, 1 = music active)
/// [0x36]        XOR checksum of bytes [0x01..0x35]
/// ```
///
/// Different scales on purpose: `bands` are AGC-normalized, so they give spectral
/// shape but never go dark in a quiet room. `level` is absolute dBFS, so it does.
/// dB SPL is dBFS + 120 with this microphone.
///
/// `level_norm` is the same loudness measured against the loudest and quietest the
/// room has been recently, so it fills 0..1 whatever the volume.
///
/// The activity flag is set by the ear chip based on sustained absolute energy
/// exceeding a calibrated threshold - the eye uses it for Auto sound mode without
/// needing to reason about absolute levels itself.
pub struct MelFrame {
    pub bands:      [u16; MEL_BANDS],
    pub level:      u16,
    pub level_norm: u16,
    pub activity:   bool,
}

impl MelFrame {
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
        buf[lvl_off + 2] = (self.level_norm & 0xFF) as u8;
        buf[lvl_off + 3] = (self.level_norm >> 8)   as u8;
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
        let level_norm = (buf[lvl_off + 2] as u16) | ((buf[lvl_off + 3] as u16) << 8);
        let activity = buf[FRAME_LEN - 2] != 0;
        Some(MelFrame { bands, level, level_norm, activity })
    }
}
