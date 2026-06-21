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
        // The mic I2S is implemented as a BIO driver (bunnie is writing the BIO
        // side), NOT the hardware UDMA I2S peripheral - so the pins are not tied
        // to a fixed alternate-function table and can be any BIO pins. Model the
        // driver on the existing bio-lib drivers (ws2812 / pulse_capture).
        //
        // BIO pins (see pins.rs): BCLK = crate::pins::MIC_BCLK_BIO_PIN (PB1),
        // SD = crate::pins::MIC_SD_BIO_PIN (PB2), WS = crate::pins::MIC_WS_BIO_PIN (PB3).
        //
        // The ICS43434 is an I2S slave; the Baochip (via BIO) acts as I2S master,
        // generating BCLK + WS. Target: 16 kHz, 24-bit, mono (IS_SELECT pin low =
        // left channel). Samples arrive left-justified in 32-bit I2S words.
        let _ = (
            crate::pins::MIC_BCLK_BIO_PIN,
            crate::pins::MIC_SD_BIO_PIN,
            crate::pins::MIC_WS_BIO_PIN,
        );
        todo!("I2S init: BIO I2S master driver (model on bio-lib ws2812/pulse_capture)")
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
