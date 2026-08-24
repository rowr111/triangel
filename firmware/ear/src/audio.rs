/// Number of samples per audio frame - the block mel.rs reduces to one MelFrame.
pub const FFT_SIZE: usize = 512;

// Every sample rate in the firmware derives from the three numbers below, so changing
// the BIO clock or the decimation cannot leave the filterbank tuned for a rate the
// hardware no longer produces.

/// BIO quantum clock, which paces the program's wait_quantum loop. The BIO toggles
/// BCLK, so BCLK is half of this.
const BIO_QUANTUM_HZ: u32 = 6_144_000;
/// SCK cycles per WS frame. Fixed at 64 by the mic: the ICS43434 requires exactly 64
/// SCK cycles in each stereo frame (datasheet, I2S Data Interface).
const BCLK_PER_FRAME: u32 = 64;
/// Decimation factor from the mic's rate down to the pipeline's.
pub const DECIMATE: usize = 3;

/// The rate the mic is clocked at - one 24-bit sample per WS frame.
pub const RAW_RATE_HZ: u32 = BIO_QUANTUM_HZ / 2 / BCLK_PER_FRAME;
/// The rate read_frame returns, after the box average that decimates each group.
pub const SAMPLE_RATE_HZ: u32 = RAW_RATE_HZ / DECIMATE as u32;
/// Wall-clock period of one FFT_SIZE frame - the budget everything downstream of
/// read_frame has to fit inside before the next frame is ready.
pub const FRAME_PERIOD_MS: u32 = FFT_SIZE as u32 * 1000 / SAMPLE_RATE_HZ;

// Integer division would quietly truncate a combination that does not divide evenly,
// leaving the filterbank tuned for a rate that never occurs.
const _: () = assert!(BIO_QUANTUM_HZ.is_multiple_of(2 * BCLK_PER_FRAME));
const _: () = assert!(RAW_RATE_HZ.is_multiple_of(DECIMATE as u32));
// The ICS43434 runs high-performance mode from 23 kHz to 51.6 kHz; between 6.25 and
// 18.75 kHz it drops to low-power mode, and below 3.125 kHz it sleeps.
const _: () = assert!(RAW_RATE_HZ >= 23_000 && RAW_RATE_HZ <= 51_600);

// --- I2S from the ICS43434 MEMS microphone (JLCPCB C5656610) ---
//
// The mic I2S is a BIO driver (the vendored program in i2s_bio.rs), not the
// hardware UDMA I2S peripheral. The Baochip is I2S master: the BIO core drives
// BCLK + WS and reads the mic's data line, pushing one right-aligned 24-bit
// left-channel sample per frame. The ICS43434 is the slave, mono (IS_SELECT tied
// low = left channel). It runs at 48 kHz (BIO quantum 6.144 MHz -> 3.072 MHz BCLK
// -> 64 BCLK/frame); read_frame downsamples 3:1 to the 16 kHz the mel pipeline
// expects (see mel.rs SAMPLE_RATE).
//
// The BIO pushes each sample to FIFO0 and read_frame polls it. The BIO checks for
// room first and drops the sample when the FIFO is full, so the clock runs
// continuously and samples are lost between frames rather than the mic being
// stopped.
mod i2s {
    use super::{BIO_QUANTUM_HZ, DECIMATE, FFT_SIZE, RAW_RATE_HZ};
    use bao1x_api::bio::*;
    use bao1x_api::bio_resources::*;
    use bao1x_api::{IoSetup, IoxDir, IoxFunction, IoxPort};
    use bao1x_hal::bio::{Bio, CoreCsr};
    use utralib::utra::bio_bdma;

    /// Raw samples discarded at startup while the mic wakes and its decimation
    /// filter settles - 100 ms. The datasheet asks for less: output begins 32768 SCK
    /// cycles after the clock starts (10.7 ms at 3.072 MHz) and is within 1 dB of
    /// settled sensitivity by 20 ms.
    const STARTUP_DISCARD: usize = RAW_RATE_HZ as usize / 10;
    /// Polls of an empty FIFO before a read gives up. Generous - it only has to
    /// tell "samples are flowing" from "nothing is arriving at all", never to
    /// time anything.
    const SAMPLE_SPIN_LIMIT: u32 = 2_000_000;

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

            // FIFO0 slot 0 fires while the level is under 7, so the BIO program can
            // test whether there is room and drop the sample rather than block. A
            // blocked push halts the core, and a halted core stops BCLK, which puts
            // the mic to sleep. Must be set before the core runs.
            bio_ss
                .setup_fifo_event_triggers(FifoEventConfig {
                    which: Fifo::Fifo0,
                    trigger_slot: TriggerSlot::new_with_raw_value(0),
                    level: FifoLevel::new_with_raw_value(7),
                    trigger_less_than: true,
                    trigger_greater_than: false,
                    trigger_equal_to: false,
                })
                .expect("couldn't set the FIFO0 room-available trigger");

            bio_ss.set_core_run_state(&resource_grant, true);

            // FIFO0 is where the BIO pushes samples. Keep the handle alongside `rx`.
            let rx_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo0) }
                .expect("FIFO0 handle error")
                .expect("no FIFO0 handle");
            let rx = CoreCsr::from_handle(&rx_handle);
            let mut this = Self { bio_ss, rx, _rx_handle: rx_handle, resource_grant };

            // The mic outputs garbage until its filter settles after the clock starts,
            // so drop the first ~100 ms of samples before any frame is read. Stops early
            // if nothing is arriving, so a silent mic reaches the caller instead of
            // spinning here forever.
            for _ in 0..STARTUP_DISCARD {
                if this.try_read_raw().is_none() {
                    break;
                }
            }
            this
        }

        /// Samples queued in FIFO0 right now (0..=8). A steady 8 means the BIO is
        /// pushing and nobody is draining; a steady 0 means nothing is arriving.
        pub fn fifo_level(&self) -> u32 {
            self.rx.csr.rf(bio_bdma::SFR_FLEVEL_PCLK_REGFIFO_LEVEL0)
        }

        // Pop one raw FIFO word, giving up after SAMPLE_SPIN_LIMIT polls of an empty
        // FIFO. The BIO pushes one right-aligned 24-bit left-channel sample per frame;
        // the FIFO is only 8 deep, so a reader must keep draining or samples are
        // dropped. None means nothing is arriving.
        pub fn try_read_raw(&mut self) -> Option<u32> {
            let mut spins = 0u32;
            while self.fifo_level() == 0 {
                spins += 1;
                if spins >= SAMPLE_SPIN_LIMIT {
                    return None;
                }
            }
            Some(self.rx.csr.r(bio_bdma::SFR_RXF0))
        }

        /// `try_read_raw` sign-extended: the low 24 bits are two's-complement, so shift
        /// the sign bit (23) up to bit 31 and arithmetic-shift back down into i32.
        pub fn try_read_sample(&mut self) -> Option<i32> {
            self.try_read_raw().map(|raw| (raw << 8) as i32 >> 8)
        }

        /// Discard everything currently queued in FIFO0, without blocking.
        pub fn flush(&mut self) {
            while self.fifo_level() != 0 {
                let _ = self.rx.csr.r(bio_bdma::SFR_RXF0);
            }
        }
    }

    impl I2sAudio {
        /// Block until a complete 512-sample frame is available, then return it.
        pub fn read_frame(&mut self) -> [i16; FFT_SIZE] {
            // Drop whatever queued while the caller processed the previous frame. Once
            // the FIFO fills the BIO discards new samples, so the eight sitting there
            // are the oldest ones from whenever it filled; flushing starts the frame on
            // fresh audio. The clock keeps running throughout, so these are ordinary
            // gaps in the stream rather than the mic being stopped and restarted.
            self.flush();

            let mut out = [0i16; FFT_SIZE];
            for slot in out.iter_mut() {
                // Downsample 48 kHz -> 16 kHz by averaging each group of DECIMATE
                // samples. The box average doubles as a cheap anti-alias low-pass; a
                // sharper FIR could replace it if aliasing artifacts appear.
                let mut acc: i32 = 0;
                for _ in 0..DECIMATE {
                    // 24-bit -> 16-bit: keep the 16 most-significant bits.
                    acc += self.try_read_sample().unwrap_or(0) >> 8;
                }
                *slot = (acc / DECIMATE as i32) as i16;
            }
            out
        }
    }
}
pub use i2s::I2sAudio;
