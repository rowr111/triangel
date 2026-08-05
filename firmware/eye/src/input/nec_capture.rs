use arbitrary_int::{Number, u5};
use bao1x_api::bio::*;
use bao1x_api::bio_resources::*;
use bao1x_api::{IoSetup, IoxDir, IoxEnable};
use bao1x_hal::bio::{Bio, CoreCsr};
use utralib::utra::bio_bdma;

use crate::input::nec_rx::nec_rx_bio_code;

// NEC infrared receive on a BIO core. The kernel halts on each RISING edge,
// classifies the interval, and pushes whole 32-bit frames - one FIFO word
// each, so the 8-deep FIFO buffers about a second of presses.

/// Pushed in place of a frame when the remote repeats (button held). Fails the
/// command checksum, so no real frame collides with it.
pub const REPEAT_FRAME: u32 = 0xFFFF_FFFF;

// Rising-edge to rising-edge intervals in microseconds: a gap plus the 560us
// burst that ends it. Nominal bit 0 = 1120, bit 1 = 2250, repeat = 2810,
// leader = 5060.
const BOUND_MIN_US:    u32 =  700; // below this: noise
const BOUND_BIT0_US:   u32 = 1685; // bit 0 | bit 1
const BOUND_BIT1_US:   u32 = 2530; // bit 1 | repeat
const BOUND_REPEAT_US: u32 = 3935; // repeat | leader
const BOUND_MAX_US:    u32 = 6500; // above this: idle between frames

pub struct NecCapture {
    bio_ss: Bio,
    // handles have to be kept around or else the underlying CSR is dropped
    _fifo_handle: CoreHandle,
    fifo: CoreCsr,
    resource_grant: ResourceGrant,
    pin: u5,
    clock_hz: u32,
}

impl Resources for NecCapture {
    fn resource_spec() -> ResourceSpec {
        ResourceSpec {
            claimer: "NecCapture".to_string(),
            cores: vec![CoreRequirement::Any],
            fifos: vec![Fifo::Fifo0],
            static_pins: vec![],
            dynamic_pin_count: 1,
        }
    }
}

impl Drop for NecCapture {
    fn drop(&mut self) {
        for &core in self.resource_grant.cores.iter() {
            self.bio_ss.de_init_core(core).unwrap();
        }
        self.bio_ss.release_dynamic_pin(self.pin.as_u8(), &Self::resource_spec().claimer).unwrap();
        self.bio_ss.release_resources(self.resource_grant.grant_id).unwrap();
    }
}

impl NecCapture {
    pub fn new(pin: u5) -> Result<Self, BioError> {
        // Schmitt-trigger input, pull-up on: the receiver idles high.
        let iox = bao1x_api::iox::IoxHal::new();
        let (port, io_pin) = bio_bit_to_port_and_pin(pin);
        iox.setup_pin(
            port,
            io_pin,
            Some(IoxDir::Input),
            Some(bao1x_api::IoxFunction::Gpio),
            Some(IoxEnable::Enable),
            Some(IoxEnable::Enable),
            None,
            None,
        );

        let mut bio_ss = Bio::new();
        let resource_grant = bio_ss.claim_resources(&Self::resource_spec())?;
        let config =
            CoreConfig { clock_mode: bao1x_api::bio::ClockMode::ExternalPin(BioPin::new(pin.as_u8())) };
        bio_ss.init_core(resource_grant.cores[0], nec_rx_bio_code(), config)?;

        bio_ss.claim_dynamic_pin(pin.as_u8(), &Self::resource_spec().claimer)?;
        // SetOnly leaves the pins other drivers have mapped alone.
        let io_config = IoConfig {
            mapped: 1 << pin.as_u32(),
            mode: IoConfigMode::SetOnly,
            ..Default::default()
        };
        bio_ss.setup_io_config(io_config).unwrap();

        bio_ss.set_core_run_state(&resource_grant, true);

        let clock_hz = bio_ss.get_bio_freq();
        let ticks = |us: u32| (us as u64 * clock_hz as u64 / 1_000_000) as u32;

        // Startup words: pin mask, then the five boundaries in ticks. Sending
        // them instead of baking them in keeps decode correct at any BIO clock.
        let fifo_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo0) }?.expect("Didn't get FIFO0 handle");
        let mut fifo = CoreCsr::from_handle(&fifo_handle);
        fifo.csr.wo(bio_bdma::SFR_TXF0, 1 << pin.as_u32());
        fifo.csr.wo(bio_bdma::SFR_TXF0, ticks(BOUND_MIN_US));
        fifo.csr.wo(bio_bdma::SFR_TXF0, ticks(BOUND_BIT0_US));
        fifo.csr.wo(bio_bdma::SFR_TXF0, ticks(BOUND_BIT1_US));
        fifo.csr.wo(bio_bdma::SFR_TXF0, ticks(BOUND_REPEAT_US));
        fifo.csr.wo(bio_bdma::SFR_TXF0, ticks(BOUND_MAX_US));

        Ok(Self { bio_ss, _fifo_handle: fifo_handle, fifo, resource_grant, pin, clock_hz })
    }

    /// BIO clock frequency the kernel's intervals are measured in.
    pub fn clock_hz(&self) -> u32 { self.clock_hz }

    /// Next frame, or None if none is waiting. Little-endian bytes are
    /// [addr lo, addr hi, cmd, ~cmd]; REPEAT_FRAME means the button is held.
    pub fn try_read_frame(&self) -> Option<u32> {
        if self.fifo.csr.rf(bio_bdma::SFR_FLEVEL_PCLK_REGFIFO_LEVEL0) == 0 {
            None
        } else {
            Some(self.fifo.csr.r(bio_bdma::SFR_RXF0))
        }
    }
}
