use bao1x_api::bio::*;
use bao1x_api::bio_resources::*;
use bao1x_api::{IoSetup, IoxDir, IoxFunction, IoxPort};
use bao1x_hal::bio::{Bio, CoreCsr};
use utralib::utra::bio_bdma;

/// Samples per audio frame.
pub const FRAME_SAMPLES: usize = 768;

// Every sample rate below derives from these three numbers.

/// BIO quantum clock. The BIO toggles BCLK on it, so BCLK is half of this.
const BIO_QUANTUM_HZ: u32 = 6_144_000;
/// SCK cycles per WS frame. The ICS43434 requires exactly 64 (datasheet, I2S Data Interface).
const BCLK_PER_FRAME: u32 = 64;
/// Decimation factor from the mic's rate down to the pipeline's.
pub const DECIMATE: usize = 2;

/// The rate the mic is clocked at - one 24-bit sample per WS frame.
pub const RAW_RATE_HZ: u32 = BIO_QUANTUM_HZ / 2 / BCLK_PER_FRAME;
/// The rate read_frame returns, after decimation.
pub const SAMPLE_RATE_HZ: u32 = RAW_RATE_HZ / DECIMATE as u32;
/// Milliseconds of audio in one frame. Frames arrive farther apart than this, because
/// audio that comes in while one is being processed is skipped.
pub const FRAME_PERIOD_MS: u32 = FRAME_SAMPLES as u32 * 1000 / SAMPLE_RATE_HZ;

// The rates must divide evenly; integer division would truncate without a warning.
const _: () = assert!(BIO_QUANTUM_HZ.is_multiple_of(2 * BCLK_PER_FRAME));
const _: () = assert!(RAW_RATE_HZ.is_multiple_of(DECIMATE as u32));
// The ICS43434's high-performance mode runs from 23 kHz to 51.6 kHz.
const _: () = assert!(RAW_RATE_HZ >= 23_000 && RAW_RATE_HZ <= 51_600);

// I2S from the ICS43434 microphone, driven by a BIO program (i2s_bio.rs). The BIO core
// drives BCLK and WS and pushes one right-aligned 24-bit left-channel sample per frame to
// FIFO0. It drops samples while the FIFO is full, so the clock never stops.

/// Raw samples discarded at startup while the mic settles: 100 ms. The datasheet says
/// it is within 1 dB of settled by 20 ms.
const STARTUP_DISCARD: usize = RAW_RATE_HZ as usize / 10;
/// Polls of an empty FIFO before a read gives up. It only has to tell flowing from stopped.
const SAMPLE_SPIN_LIMIT: u32 = 2_000_000;

pub struct I2sAudio {
    bio_ss: Bio,
    // CoreCsr view of FIFO0, where the BIO program pushes mic samples.
    rx: CoreCsr,
    // The handle must outlive `rx` or the underlying CSR mapping is dropped.
    _rx_handle: CoreHandle,
    resource_grant: ResourceGrant,
    /// Frames abandoned because nothing was arriving.
    starved_frames: u32,
}

impl Resources for I2sAudio {
    fn resource_spec() -> ResourceSpec {
        ResourceSpec {
            claimer: "i2s-mic".to_string(),
            cores: vec![CoreRequirement::Any],
            fifos: vec![Fifo::Fifo0],
            // BCLK, SD and WS are fixed in the BIO program (i2s_bio.rs).
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
        // BIO bit N is PB N on this board, so the port is PB and the pin is the bit number.
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

        // FIFO0 slot 0 fires while the level is under 7, so the BIO program can test for
        // room and drop the sample instead of blocking. A blocked push halts the core,
        // which stops BCLK and puts the mic to sleep. Must be set before the core runs.
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

        let rx_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo0) }
            .expect("FIFO0 handle error")
            .expect("no FIFO0 handle");
        let rx = CoreCsr::from_handle(&rx_handle);
        let mut this =
            Self { bio_ss, rx, _rx_handle: rx_handle, resource_grant, starved_frames: 0 };

        // Drop the first samples while the mic settles. Stops early if nothing is
        // arriving, so a silent mic does not hang here.
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

    /// Pop one raw FIFO word, or None after SAMPLE_SPIN_LIMIT polls of an empty FIFO.
    /// The FIFO is 8 deep, so a reader must keep draining or samples are dropped.
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

    /// `try_read_raw`, sign-extended from 24 bits.
    pub fn try_read_sample(&mut self) -> Option<i32> {
        self.try_read_raw().map(|raw| (raw << 8) as i32 >> 8)
    }

    /// Frames abandoned so far because nothing was arriving.
    pub fn starved_frames(&self) -> u32 { self.starved_frames }

    /// Discard everything currently queued in FIFO0, without blocking.
    pub fn flush(&mut self) {
        while self.fifo_level() != 0 {
            let _ = self.rx.csr.r(bio_bdma::SFR_RXF0);
        }
    }

    /// Block until a complete frame is available, then return it.
    pub fn read_frame(&mut self) -> [i16; FRAME_SAMPLES] {
        // Drop what queued while the caller processed the last frame, so this one starts
        // on fresh audio.
        self.flush();

        let mut out = [0i16; FRAME_SAMPLES];
        for slot in out.iter_mut() {
            // Average each group of DECIMATE samples: 48 kHz down to 24 kHz. The average
            // also acts as a simple low-pass against aliasing.
            let mut acc: i32 = 0;
            for _ in 0..DECIMATE {
                let Some(s) = self.try_read_sample() else {
                    // Give up rather than pay the spin limit for every remaining sample.
                    self.starved_frames = self.starved_frames.wrapping_add(1);
                    return out;
                };
                // 24-bit -> 16-bit: keep the 16 most-significant bits.
                acc += s >> 8;
            }
            *slot = (acc / DECIMATE as i32) as i16;
        }
        out
    }
}
