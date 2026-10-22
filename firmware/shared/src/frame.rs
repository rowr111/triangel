/// Baud rate of the ear-to-eye UART link.
pub const EAR_UART_BAUD: u32 = 1_000_000;

/// First byte of every frame.
pub const SYNC_BYTE: u8 = 0xAA;

/// Number of frequency bands the ear chip computes.
pub const BAND_COUNT: usize = 24;

/// Quietest level the wire carries. LEVEL_DB_FLOOR..0 dBFS maps onto 0..=65535.
pub const LEVEL_DB_FLOOR: f32 = -90.0;

/// Where a dBFS level sits between LEVEL_DB_FLOOR and 0, as 0.0-1.0. Not clamped.
pub fn level_to_norm(dbfs: f32) -> f32 {
    (dbfs - LEVEL_DB_FLOOR) / -LEVEL_DB_FLOOR
}

/// Encode dBFS for the wire.
pub fn level_to_wire(dbfs: f32) -> u16 {
    norm_to_wire(level_to_norm(dbfs))
}

/// Decode a wire level back to absolute dBFS.
pub fn level_from_wire(level: u16) -> f32 {
    LEVEL_DB_FLOOR + norm_from_wire(level) * -LEVEL_DB_FLOOR
}

/// Encode a 0.0-1.0 level for the wire.
pub fn norm_to_wire(norm: f32) -> u16 {
    (norm.clamp(0.0, 1.0) * 65535.0) as u16
}

/// Decode a wire level back to 0.0-1.0.
pub fn norm_from_wire(norm: u16) -> f32 {
    norm as f32 / 65535.0
}

/// The u16 values after the bands: level, level_norm, flux, bass.
const SCALARS: usize = 4;

/// Wire frame length in bytes: a sync byte, the u16 values, a checksum byte.
pub const FRAME_LEN: usize = 1 + (BAND_COUNT + SCALARS) * 2 + 1; // 58 bytes

/// One frame from the ear to the eye. On the wire, little-endian: the sync byte, then
/// these fields in order as u16s, then an XOR checksum.
pub struct BandFrame {
    /// Normalized against recent music: spectral shape, never dark in a quiet room.
    pub bands:      [u16; BAND_COUNT],
    /// Overall loudness in absolute dBFS. dB SPL is roughly dBFS + 120 with this microphone.
    pub level:      u16,
    /// Loudness against the loudest and quietest the room has been recently, 0..1.
    pub level_norm: u16,
    /// How much the whole spectrum rose this frame, before smoothing or normalizing.
    pub flux:       u16,
    /// The lowest bands' level in dBFS, before normalizing, so the eye can see the bass leave.
    pub bass:       u16,
}

impl BandFrame {
    /// Write the frame into a wire buffer.
    pub fn encode(&self, buf: &mut [u8; FRAME_LEN]) {
        buf[0] = SYNC_BYTE;
        let scalars: [u16; SCALARS] = [self.level, self.level_norm, self.flux, self.bass];
        let words = self.bands.iter().chain(&scalars);
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
        let [level, level_norm, flux, bass]: [u16; SCALARS] =
            std::array::from_fn(|i| word(BAND_COUNT + i));
        Some(BandFrame { bands: std::array::from_fn(word), level, level_norm, flux, bass })
    }
}

/// What one byte did to a `FrameAssembler`.
pub enum Fed {
    /// No complete frame yet.
    Pending,
    Frame(BandFrame),
    /// A full frame's worth of bytes failed its checksum.
    Bad,
}

/// Rebuilds `BandFrame`s from the UART byte stream, one byte at a time.
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
        match BandFrame::decode(&self.buf) {
            Some(frame) => Fed::Frame(frame),
            None => {
                // A frame may have started inside this one: keep from the next sync byte on.
                if let Some(i) = self.buf[1..].iter().position(|&b| b == SYNC_BYTE) {
                    self.buf.copy_within(1 + i.., 0);
                    self.pos = FRAME_LEN - 1 - i;
                }
                Fed::Bad
            }
        }
    }
}

/// XOR of every byte between the sync byte and the checksum byte.
fn checksum(buf: &[u8; FRAME_LEN]) -> u8 {
    buf[1..FRAME_LEN - 1].iter().fold(0, |acc, &b| acc ^ b)
}
