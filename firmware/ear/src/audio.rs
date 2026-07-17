/// Number of samples per audio frame - matches the FFT size in mel.rs.
pub const FFT_SIZE: usize = 512;

pub trait AudioSource {
    /// Block until a complete 512-sample frame is available, then return it.
    fn read_frame(&mut self) -> [i16; FFT_SIZE];
}

// --- uart-audio: PCM over USB serial from ear_sim.py ---
#[cfg(feature = "uart-audio")]
mod uart {
    use super::{AudioSource, FFT_SIZE};

    const PCM_SYNC: u8 = 0xBB;
    const PACKET_BYTES: usize = 1 + FFT_SIZE * 2; // 1025: sync + 512 x i16 LE

    pub struct UartAudio {
        usb: usb_bao1x::UsbHid,
        buf: Vec<u8>,
    }

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
}
#[cfg(feature = "uart-audio")]
pub use uart::UartAudio;

// --- production: I2S from ICS43434 MEMS microphone (JLCPCB C5656610) ---
//
// The mic I2S is a BIO driver (the vendored program in i2s_bio.rs), not the
// hardware UDMA I2S peripheral. The Baochip is I2S master: the BIO core drives
// BCLK + WS and reads the mic's data line, pushing one right-aligned 24-bit
// left-channel sample per frame into FIFO0. The ICS43434 is the slave, mono
// (IS_SELECT tied low = left channel). It runs at 48 kHz (BIO quantum 6.144 MHz
// -> 3.072 MHz BCLK -> 64 BCLK/frame); read_frame downsamples 3:1 to the 16 kHz
// the mel pipeline expects (see mel.rs SAMPLE_RATE).
#[cfg(not(feature = "uart-audio"))]
mod i2s {
    use super::{AudioSource, FFT_SIZE};
    use bao1x_api::bio::*;
    use bao1x_api::bio_resources::*;
    use bao1x_api::{IoSetup, IoxDir, IoxFunction, IoxPort};
    use bao1x_hal::bio::{Bio, CoreCsr};
    use utralib::utra::bio_bdma;

    /// BIO quantum clock; paces the program's wait_quantum loop to yield 48 kHz.
    const BIO_QUANTUM_HZ: u32 = 6_144_000;
    /// Decimation factor from the 48 kHz mic to the 16 kHz pipeline.
    const DECIMATE: usize = 3;

    pub struct I2sAudio {
        bio_ss: Bio,
        // CoreCsr view of FIFO0, where the BIO program pushes mic samples.
        rx: CoreCsr,
        // The handle must outlive `rx` or the underlying CSR mapping is dropped.
        _rx_handle: CoreHandle,
        resource_grant: ResourceGrant,
    }

    impl Resources for I2sAudio {
        fn resource_spec() -> ResourceSpec {
            ResourceSpec {
                claimer: "i2s-mic".to_string(),
                cores: vec![CoreRequirement::Any],
                fifos: vec![Fifo::Fifo0],
                // BCLK/SD/WS are fixed in the BIO program (i2s_bio.rs); see pins.rs.
                static_pins: vec![
                    crate::pins::MIC_BCLK_BIO_PIN,
                    crate::pins::MIC_SD_BIO_PIN,
                    crate::pins::MIC_WS_BIO_PIN,
                ],
                dynamic_pin_count: 0,
            }
        }
    }

    impl Drop for I2sAudio {
        fn drop(&mut self) {
            for &core in self.resource_grant.cores.iter() {
                self.bio_ss.de_init_core(core).unwrap();
            }
            self.bio_ss.release_resources(self.resource_grant.grant_id).unwrap();
        }
    }

    impl I2sAudio {
        pub fn new() -> Self {
            // Configure the three mic pins on the IO mux. BIO bit N maps to PB N on
            // this board (see pins.rs), so the port is PB and the pin is the bit
            // number. BCLK + WS are outputs the BIO drives; SD is the mic data input.
            let iox = bao1x_api::iox::IoxHal::new();
            for (pin, dir) in [
                (crate::pins::MIC_BCLK_BIO_PIN, IoxDir::Output),
                (crate::pins::MIC_WS_BIO_PIN, IoxDir::Output),
                (crate::pins::MIC_SD_BIO_PIN, IoxDir::Input),
            ] {
                iox.setup_pin(IoxPort::PB, pin, Some(dir), Some(IoxFunction::Gpio), None, None, None, None);
            }

            let mut bio_ss = Bio::new();
            let spec = Self::resource_spec();
            let resource_grant = bio_ss.claim_resources(&spec).expect("couldn't claim BIO resources for I2S");

            let config = CoreConfig { clock_mode: ClockMode::TargetFreqInt(BIO_QUANTUM_HZ) };
            bio_ss
                .init_core(resource_grant.cores[0], crate::i2s_bio::i2s_bio_code(), config)
                .expect("couldn't init I2S BIO core");

            // Route the three pins to the BIO before the core starts driving them.
            let io_config = IoConfig {
                mapped: (1u32 << crate::pins::MIC_BCLK_BIO_PIN)
                    | (1u32 << crate::pins::MIC_SD_BIO_PIN)
                    | (1u32 << crate::pins::MIC_WS_BIO_PIN),
                mode: IoConfigMode::Overwrite,
                ..Default::default()
            };
            bio_ss.setup_io_config(io_config).unwrap();

            bio_ss.set_core_run_state(&resource_grant, true);

            // FIFO0 is where the BIO pushes samples. Keep the handle alongside `rx`.
            let rx_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo0) }
                .expect("FIFO0 handle error")
                .expect("no FIFO0 handle");
            let rx = CoreCsr::from_handle(&rx_handle);

            Self { bio_ss, rx, _rx_handle: rx_handle, resource_grant }
        }

        // Pop one 24-bit sample, blocking until FIFO0 is non-empty. The BIO pushes one
        // right-aligned 24-bit left-channel sample per frame; the FIFO is only 8 deep,
        // so read_frame must keep draining or the BIO stalls on a full FIFO.
        fn read_sample(&mut self) -> i32 {
            while self.rx.csr.rf(bio_bdma::SFR_FLEVEL_PCLK_REGFIFO_LEVEL0) == 0 {}
            let raw = self.rx.csr.r(bio_bdma::SFR_RXF0);
            // 24-bit two's-complement in the low 24 bits: shift the sign bit (23) up to
            // bit 31, then arithmetic-shift back down to sign-extend into i32.
            (raw << 8) as i32 >> 8
        }
    }

    impl AudioSource for I2sAudio {
        fn read_frame(&mut self) -> [i16; FFT_SIZE] {
            let mut out = [0i16; FFT_SIZE];
            for slot in out.iter_mut() {
                // Downsample 48 kHz -> 16 kHz by averaging each group of DECIMATE
                // samples. The box average doubles as a cheap anti-alias low-pass; a
                // sharper FIR could replace it if aliasing artifacts appear.
                let mut acc: i32 = 0;
                for _ in 0..DECIMATE {
                    // 24-bit -> 16-bit: keep the 16 most-significant bits.
                    acc += self.read_sample() >> 8;
                }
                *slot = (acc / DECIMATE as i32) as i16;
            }
            out
        }
    }
}
#[cfg(not(feature = "uart-audio"))]
pub use i2s::I2sAudio;

// Feature-selected alias so main.rs needs no cfg blocks for the audio source type.
#[cfg(feature = "uart-audio")]
pub type ActiveAudio = UartAudio;

#[cfg(not(feature = "uart-audio"))]
pub type ActiveAudio = I2sAudio;
