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
    norm_to_wire((dbfs - LEVEL_DB_FLOOR) / -LEVEL_DB_FLOOR)
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
/// + 2 flux + 2 bass + 1 checksum.
pub const FRAME_LEN: usize = 1 + MEL_BANDS * 2 + 2 + 2 + 2 + 2 + 1; // 58 bytes

/// One frame of mel band data sent from the ear chip to the eye chip.
///
/// Wire format (58 bytes, little-endian):
///
/// ```text
/// [0x00]        SYNC_BYTE (0xAA)
/// [0x01..0x30]  bands[0..23] as u16 little-endian  (48 bytes)
/// [0x31..0x32]  level as u16 little-endian          (2 bytes)
/// [0x33..0x34]  level_norm as u16 little-endian     (2 bytes)
/// [0x35..0x36]  flux as u16 little-endian           (2 bytes)
/// [0x37..0x38]  bass as u16 little-endian           (2 bytes)
/// [0x39]        XOR checksum of bytes [0x01..0x38]
/// ```
///
/// Different scales on purpose: `bands` are AGC-normalized, so they give spectral
/// shape but never go dark in a quiet room. `level` is absolute dBFS, so it does.
/// dB SPL is dBFS + 120 with this microphone.
///
/// `level_norm` is the same loudness measured against the loudest and quietest the
/// room has been recently, so it fills 0..1 whatever the volume.
///
/// `flux` is how much the whole spectrum rose this frame, measured before the bands are
/// smoothed or normalized. A struck drum moves every band at once and spikes it.
///
/// `bass` is the lowest bands' level in dBFS, encoded like `level` and taken before any
/// normalization. The bands above it cannot show the bass going away, since each one's
/// reference sinks to meet the quiet.
pub struct MelFrame {
    pub bands:      [u16; MEL_BANDS],
    pub level:      u16,
    pub level_norm: u16,
    pub flux:       u16,
    pub bass:       u16,
}

impl MelFrame {
    /// Serialise into a wire buffer.
    pub fn encode(&self, buf: &mut [u8; FRAME_LEN]) {
        buf[0] = SYNC_BYTE;
        let words = self.bands.iter().chain([&self.level, &self.level_norm, &self.flux, &self.bass]);
        for (bytes, word) in buf[1..FRAME_LEN - 1].chunks_exact_mut(2).zip(words) {
            bytes.copy_from_slice(&word.to_le_bytes());
        }
        buf[FRAME_LEN - 1] = checksum(buf);
    }

    /// Parse a wire buffer. Returns `None` if sync or checksum is wrong.
    pub fn decode(buf: &[u8; FRAME_LEN]) -> Option<Self> {
        if buf[0] != SYNC_BYTE || buf[FRAME_LEN - 1] != checksum(buf) {
            return None;
        }
        let word = |i: usize| u16::from_le_bytes([buf[1 + i * 2], buf[2 + i * 2]]);
        Some(MelFrame {
            bands:      std::array::from_fn(word),
            level:      word(MEL_BANDS),
            level_norm: word(MEL_BANDS + 1),
            flux:       word(MEL_BANDS + 2),
            bass:       word(MEL_BANDS + 3),
        })
    }
}

/// What one byte did to a `FrameAssembler`.
pub enum Fed {
    /// No complete frame yet.
    Pending,
    Frame(MelFrame),
    /// A full frame's worth of bytes failed its checksum.
    Bad,
}

/// Rebuilds `MelFrame`s from the UART byte stream, one byte at a time.
pub struct FrameAssembler {
    buf: [u8; FRAME_LEN],
    pos: usize,
}

impl Default for FrameAssembler {
    fn default() -> Self {
        Self { buf: [0; FRAME_LEN], pos: 0 }
    }
}

impl FrameAssembler {
    pub fn feed(&mut self, byte: u8) -> Fed {
        if self.pos == 0 {
            // Hunt for the sync byte; ignore anything else.
            if byte == SYNC_BYTE {
                self.buf[0] = byte;
                self.pos = 1;
            }
            return Fed::Pending;
        }
        self.buf[self.pos] = byte;
        self.pos += 1;
        if self.pos < FRAME_LEN {
            return Fed::Pending;
        }
        // Full frame collected. Reset for the next one, then validate.
        self.pos = 0;
        match MelFrame::decode(&self.buf) {
            Some(frame) => Fed::Frame(frame),
            None => Fed::Bad,
        }
    }
}

/// XOR of every byte between the sync byte and the checksum byte.
fn checksum(buf: &[u8; FRAME_LEN]) -> u8 {
    buf[1..FRAME_LEN - 1].iter().fold(0, |acc, &b| acc ^ b)
}
