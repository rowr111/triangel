use arbitrary_int::{Number, u5};
use bao1x_api::bio::*;
use bao1x_api::bio_code;
use bao1x_api::bio_resources::*;
use bao1x_api::{IoSetup, IoxDir, IoxEnable};
use bao1x_hal::bio::{Bio, CoreCsr};
use utralib::utra::bio_bdma;

// NEC infrared receive on a BIO core. The core halts on each RISING edge of
// the pin (the end of each burst), measures the interval since the previous
// one, classifies it, and assembles whole 32-bit frames. Only finished frames
// reach the CPU, one FIFO word each, so the 8-deep FIFO holds eight frames -
// about a second at the protocol's repeat rate - and CPU scheduling cannot
// cost us a frame.

/// Pushed in place of a frame when the remote sends a repeat (button held).
/// Not a valid frame: its command byte pair fails the protocol's checksum.
pub const REPEAT_FRAME: u32 = 0xFFFF_FFFF;

// Rising-edge to rising-edge intervals, in microseconds: one gap plus the
// 560us burst that ends it. Nominal values are bit 0 = 1120, bit 1 = 2250,
// repeat = 2810, leader = 5060. The kernel classifies by which pair of
// boundaries an interval falls between.
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
        // Schmitt-trigger input with the pull-up on, suiting the receiver's
        // idle-high output.
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
        bio_ss.init_core(resource_grant.cores[0], nec_capture_code(), config)?;

        bio_ss.claim_dynamic_pin(pin.as_u8(), &Self::resource_spec().claimer)?;
        // SetOnly maps this pin without disturbing pins other drivers have mapped.
        let io_config = IoConfig {
            mapped: 1 << pin.as_u32(),
            mode: IoConfigMode::SetOnly,
            ..Default::default()
        };
        bio_ss.setup_io_config(io_config).unwrap();

        bio_ss.set_core_run_state(&resource_grant, true);

        let clock_hz = bio_ss.get_bio_freq();
        let ticks = |us: u32| (us as u64 * clock_hz as u64 / 1_000_000) as u32;

        // Startup: the pin mask, then the four boundaries in ticks. Sending
        // them (rather than baking them into the kernel) keeps the decode
        // correct whatever the BIO clock turns out to be.
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

    /// Next received frame, or None if none is waiting. Bytes read
    /// little-endian are [address low, address high, command, ~command];
    /// REPEAT_FRAME means the previous button is being held.
    pub fn try_read_frame(&self) -> Option<u32> {
        if self.fifo.csr.rf(bio_bdma::SFR_FLEVEL_PCLK_REGFIFO_LEVEL0) == 0 {
            None
        } else {
            Some(self.fifo.csr.r(bio_bdma::SFR_RXF0))
        }
    }
}

#[rustfmt::skip]
bio_code!(
    nec_capture_code,
    NEC_CAPTURE_START,
    NEC_CAPTURE_END,

    // x1..x4, x12: interval boundaries, ascending
    // x5: pin mask   x6: previous timestamp   x7: this timestamp   x8: interval
    // x9: core-ID mask   x10: bits so far   x11: bit count
    // x13: scratch   x14: 0x80000000   x15: 1 while assembling a frame
    "mv    x5, x16",         // pin mask from FIFO0 (blocks until the host sends it)
    "mv    x26, x5",         // set GPIO mask to our pin
    "mv    x25, x5",         // configure pin as an input
    "mv    x1, x16",         // boundaries, in BIO clock ticks
    "mv    x2, x16",
    "mv    x3, x16",
    "mv    x4, x16",
    "mv    x12, x16",

    "li    x9, 0x3FFFFFFF",  // timestamp bits below the core ID
    "lui   x14, 0x80000",    // 0x80000000 - the bit position a 1 shifts in at
    "li    x15, 0",          // start idle
    "li    x10, 0",
    "li    x11, 0",
    "mv    x6, x31",         // seed the previous-edge timestamp

"10:", // one iteration per rising edge
    "mv    x20, x0",         // wait for quantum = rising edge on the configured pin
    "mv    x7, x31",         // timestamp this edge
    "and   x6, x6, x9",      // mask out core ID bits from both timestamps
    "and   x7, x7, x9",
    "bgtu  x6, x7, 20f",     // roll-over if the previous timestamp is the larger
    "sub   x8, x7, x6",      // interval is x7 - x6
    "j     30f",
"20:", // roll-over path
    "sub   x13, x9, x6",     // ticks from the previous edge to the maximum count
    "add   x8, x13, x7",     // plus the ticks since roll-over
"30:",
    "mv    x6, x7",          // this edge becomes the previous edge

    // classify the interval
    "bltu  x8, x1, 90f",     // shorter than any symbol: noise
    "bltu  x8, x2, 40f",     // bit 0
    "bltu  x8, x3, 50f",     // bit 1
    "bltu  x8, x4, 60f",     // repeat frame
    "bltu  x8, x12, 70f",    // leader
    "j     90f",             // longer than a leader: idle between frames

"40:", // bit 0 - shift a zero in at the top, LSB first on the wire
    "beqz  x15, 10b",        // ignore unless assembling a frame
    "srli  x10, x10, 1",
    "j     80f",

"50:", // bit 1
    "beqz  x15, 10b",
    "srli  x10, x10, 1",
    "or    x10, x10, x14",
    "j     80f",

"60:", // repeat frame - only meaningful between frames
    "bnez  x15, 90f",
    "li    x13, -1",         // REPEAT_FRAME
    "mv    x16, x13",        // push to the host
    "j     10b",

"70:", // leader - start a fresh frame whatever we were doing
    "li    x15, 1",
    "li    x10, 0",
    "li    x11, 0",
    "j     10b",

"80:", // count the bit, push the frame once 32 are in
    "addi  x11, x11, 1",
    "li    x13, 32",
    "bne   x11, x13, 10b",
    "mv    x16, x10",        // push to the host
    "li    x15, 0",
    "j     10b",

"90:", // anything unexpected: wait for the next leader
    "li    x15, 0",
    "j     10b"
);
