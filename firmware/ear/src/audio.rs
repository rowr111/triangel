/// Number of samples per audio frame - matches the FFT size in mel.rs.
pub const FFT_SIZE: usize = 512;

#[cfg(feature = "uart-audio")]
const PCM_SYNC: u8 = 0xBB;
#[cfg(feature = "uart-audio")]
const PACKET_BYTES: usize = 1 + FFT_SIZE * 2; // 1025: sync + 512 x i16 LE

pub trait AudioSource {
    /// Block until a complete 512-sample frame is available, then return it.
    fn read_frame(&mut self) -> [i16; FFT_SIZE];
}

// --- uart-audio: PCM over USB serial from ear_sim.py ---

#[cfg(feature = "uart-audio")]
pub struct UartAudio {
    usb: usb_bao1x::UsbHid,
    buf: Vec<u8>,
}

#[cfg(feature = "uart-audio")]
impl UartAudio {
    pub fn new() -> Self {
        let usb = usb_bao1x::UsbHid::new();
        Self { usb, buf: Vec::new() }
    }

    fn fill_buf(&mut self) {
        // serial_wait_binary() blocks until at least some bytes arrive on the
        // USB CDC serial port, then returns them as a Vec<u8>. No polling loop
        // needed - the Xous USB server wakes us when data is ready.
        let bytes = self.usb.serial_wait_binary();
        self.buf.extend_from_slice(&bytes);
    }
}

#[cfg(feature = "uart-audio")]
impl AudioSource for UartAudio {
    fn read_frame(&mut self) -> [i16; FFT_SIZE] {
        loop {
            // Ensure there is at least one byte to inspect
            while self.buf.is_empty() {
                self.fill_buf();
            }

            // Locate the 0xBB sync byte; discard anything before it
            let sync_pos = match self.buf.iter().position(|&b| b == PCM_SYNC) {
                Some(p) => p,
                None => {
                    self.buf.clear();
                    continue;
                }
            };
            self.buf.drain(..sync_pos);

            // Wait for a full packet
            while self.buf.len() < PACKET_BYTES {
                self.fill_buf();
            }

            // Parse 512 i16 LE samples from payload (bytes 1..1025)
            let mut frame = [0i16; FFT_SIZE];
            for (i, chunk) in self.buf[1..PACKET_BYTES].chunks_exact(2).enumerate() {
                frame[i] = i16::from_le_bytes([chunk[0], chunk[1]]);
            }
            self.buf.drain(..PACKET_BYTES);
            return frame;
        }
    }
}

// --- production: I2S from ICS43434 MEMS microphone (JLCPCB C5656610) ---

#[cfg(not(feature = "uart-audio"))]
pub struct I2sAudio;

#[cfg(not(feature = "uart-audio"))]
impl I2sAudio {
    pub fn new() -> Self {
        // No I2S HAL exists in bao1x-hal yet (no udma/i2s.rs). The existing
        // xous-core codec service wraps I2S but is hardwired to a TLV320AIC3100
        // codec chip at 8 kHz stereo - not suitable for the ICS43434 MEMS mic.
        //
        // Implementation will need raw UDMA I2S register access via utralib:
        //   HW_UDMA_I2S_BASE     = 0x5010_e000
        //   REG_I2S_MST_SETUP    - master mode: sample rate, bit depth, channel count
        //   REG_I2S_CLKCFG_SETUP - clock divider (derive from PERCLK_HZ = 100 MHz)
        //
        // The codec service's audio_handler ISR (services/codec/src/backend/
        // tlv320aic3100.rs) is a useful reference for the interrupt-driven FIFO
        // read pattern, even though the chip-specific parts don't apply here.
        //
        // The ICS43434 is an I2S slave; the Baochip acts as I2S master.
        // Target: 16 kHz, 24-bit, mono (IS_SELECT pin low = left channel).
        // Samples arrive left-justified in 32-bit I2S words.
        todo!("I2S init: write raw UDMA I2S registers via utralib - see codec service as reference")
    }
}

#[cfg(not(feature = "uart-audio"))]
impl AudioSource for I2sAudio {
    fn read_frame(&mut self) -> [i16; FFT_SIZE] {
        // TODO: read FFT_SIZE samples from the I2S UDMA RX buffer.
        // The ICS43434 delivers 24-bit samples left-justified in 32-bit I2S words.
        // Shift right by 8 to get the top 24 bits, then cast to i16 (drops the
        // bottom 8 bits of the 24, keeping the 16 most significant):
        //   let sample_i16 = (raw_i32 >> 8) as i16;
        todo!("I2S frame read: drain UDMA I2S RX buffer into [i16; FFT_SIZE]")
    }
}

// Feature-selected alias so main.rs needs no cfg blocks for the audio source type.
#[cfg(feature = "uart-audio")]
pub type ActiveAudio = UartAudio;

#[cfg(not(feature = "uart-audio"))]
pub type ActiveAudio = I2sAudio;
