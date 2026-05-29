/// Baud rate for the ear->eye UART link. Must match on both chips.
pub const EAR_UART_BAUD: u32 = 1_000_000;

/// Sync byte that starts every mel frame on the ear->eye UART wire.
pub const SYNC_BYTE: u8 = 0xAA;

/// Number of mel frequency bands the ear chip computes.
pub const MEL_BANDS: usize = 24;

/// Wire frame length in bytes: 1 sync + MEL_BANDS*2 data + 1 activity + 1 checksum.
pub const FRAME_LEN: usize = 1 + MEL_BANDS * 2 + 1 + 1; // 51 bytes

/// One frame of mel band data sent from the ear chip to the eye chip.
///
/// Wire format (51 bytes, little-endian):
///
/// ```text
/// [0x00]        SYNC_BYTE (0xAA)
/// [0x01..0x30]  bands[0..23] as u16 little-endian  (48 bytes)
/// [0x31]        activity flag (0 = quiet, 1 = music active)
/// [0x32]        XOR checksum of bytes [0x01..0x31]
/// ```
///
/// Band values are scaled so that 0 = silence and 65535 = full scale.
/// The eye chip divides by 65535.0 to get a 0.0-1.0 float.
///
/// The activity flag is set by the ear chip based on sustained absolute energy
/// exceeding a calibrated threshold - the eye uses it for Auto sound mode without
/// needing to reason about absolute levels itself.
pub struct MelFrame {
    pub bands:    [u16; MEL_BANDS],
    pub activity: bool,
}

impl MelFrame {
    /// Serialise into a 51-byte wire buffer.
    pub fn encode(&self, buf: &mut [u8; FRAME_LEN]) {
        buf[0] = SYNC_BYTE;
        for (i, &band) in self.bands.iter().enumerate() {
            let off = 1 + i * 2;
            buf[off]     = (band & 0xFF) as u8;
            buf[off + 1] = (band >> 8)   as u8;
        }
        buf[FRAME_LEN - 2] = self.activity as u8;
        let checksum = buf[1..FRAME_LEN - 1].iter().fold(0u8, |acc, &b| acc ^ b);
        buf[FRAME_LEN - 1] = checksum;
    }

    /// Parse a 51-byte wire buffer. Returns `None` if sync or checksum is wrong.
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
        let activity = buf[FRAME_LEN - 2] != 0;
        Some(MelFrame { bands, activity })
    }
}
