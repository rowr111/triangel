use arbitrary_int::{Number, u5};
use bao1x_api::bio::*;
use bao1x_api::bio_code;
use bao1x_api::bio_resources::*;
use bao1x_hal::bio::{Bio, CoreCsr};
use utralib::utra::bio_bdma;

use crate::led::map::LED_COUNT;

// One WS2812B chain driven from a single BIO core, carrying all 600 LEDs.
//
// This is bio-lib's ws2812 driver with one change: the kernel's pixel buffer
// starts lower in the core's memory. A BIO core has 4096 bytes total, shared
// between the kernel code and the buffer, and bio-lib starts the buffer at byte
// 2048 - which caps a chain at 512 LEDs, short of the 600 here. The kernel is
// about 240 bytes, so starting at byte 1024 still leaves it four times the room
// it needs while raising the cap to 768 LEDs.

/// Byte offset in the core's memory where the pixel buffer starts. Must match
/// the three `li` instructions in the kernel below.
const BUFFER_START: usize = 0x400;
/// The core's total memory, code and pixel buffer together.
const CORE_MEM_BYTES: usize = 4096;
/// Longest chain the buffer can hold, at 4 bytes per LED.
pub const MAX_LEDS: usize = (CORE_MEM_BYTES - BUFFER_START) / 4;

const _: () = assert!(
    LED_COUNT <= MAX_LEDS,
    "LED_COUNT exceeds the BIO core's pixel buffer; lower BUFFER_START (and the kernel's li instructions) to fit"
);

/// 150ns quantum - the WS2812B bit timings below are counted in these.
const QUANTUM_HZ: u32 = 6_666_667;

pub struct Ws2812 {
    bio_ss: Bio,
    pin: u5,
    // handles have to be kept around or else the underlying CSR is dropped
    _tx_handle: CoreHandle,
    _rx_handle: CoreHandle,
    tx: CoreCsr,
    rx: CoreCsr,
    resource_grant: ResourceGrant,
}

impl Resources for Ws2812 {
    fn resource_spec() -> ResourceSpec {
        ResourceSpec {
            claimer: "Ws2812".to_string(),
            cores: vec![CoreRequirement::Any],
            fifos: vec![Fifo::Fifo1, Fifo::Fifo2],
            static_pins: vec![],
            dynamic_pin_count: 1,
        }
    }
}

impl Drop for Ws2812 {
    fn drop(&mut self) {
        for &core in self.resource_grant.cores.iter() {
            self.bio_ss.de_init_core(core).unwrap();
        }
        self.bio_ss.release_dynamic_pin(self.pin.as_u8(), &Ws2812::resource_spec().claimer).unwrap();
        self.bio_ss.release_resources(self.resource_grant.grant_id).unwrap();
    }
}

impl Ws2812 {
    pub fn new(pin: u5) -> Result<Self, BioError> {
        let mut bio_ss = Bio::new();
        let resource_grant = bio_ss.claim_resources(&Self::resource_spec())?;
        let config = CoreConfig { clock_mode: ClockMode::TargetFreqInt(QUANTUM_HZ) };
        bio_ss.init_core(resource_grant.cores[0], ws2812b_kernel(), config)?;
        bio_ss.set_core_run_state(&resource_grant, true);

        bio_ss.claim_dynamic_pin(pin.as_u8(), &Self::resource_spec().claimer)?;
        // SetOnly leaves the pins other drivers have mapped alone; Overwrite would
        // unmap the IR receiver's pin.
        let io_config = IoConfig {
            mapped: 1 << pin.as_u32(),
            mode: IoConfigMode::SetOnly,
            ..Default::default()
        };
        bio_ss.setup_io_config(io_config).unwrap();

        // safety: the handles are stored in the struct so they outlive the CoreCsr views
        let tx_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo1) }?.expect("Didn't get FIFO1 handle");
        let rx_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo2) }?.expect("Didn't get FIFO2 handle");
        let mut tx = CoreCsr::from_handle(&tx_handle);

        // First word the kernel reads is the pin mask it drives.
        tx.csr.wo(bio_bdma::SFR_TXF1, io_config.mapped);

        Ok(Self {
            bio_ss,
            pin,
            tx,
            rx: CoreCsr::from_handle(&rx_handle),
            _tx_handle: tx_handle,
            _rx_handle: rx_handle,
            resource_grant,
        })
    }

    /// Hands the strip to the core and returns without waiting. Bit 24 of the last
    /// word tells the core the strip is complete and transmission may start.
    pub fn send_async(&mut self, strip: &[u32]) {
        if let Some((&last, elements)) = strip.split_last() {
            for &led in elements.iter() {
                self.tx.csr.wo(bio_bdma::SFR_TXF1, led);
            }
            self.tx.csr.wo(bio_bdma::SFR_TXF1, last | 0x1_00_00_00);
        }
    }

    /// Waits for the strip to finish. Skipping this before the next `send_async`
    /// risks overflowing the FIFO while the core is busy.
    pub fn send_await(&self) {
        while self.rx.csr.rf(bio_bdma::SFR_FLEVEL_PCLK_REGFIFO_LEVEL2) == 0 {
            xous::yield_slice();
        }
        let _token = self.rx.csr.r(bio_bdma::SFR_RXF2); // empty the token
    }
}

/// Packs RGB into the GRB word order the WS2812 expects.
pub fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 { ((g as u32) << 16) | ((r as u32) << 8) | (b as u32) }

// WS2812B kernel
//
// FIFO1 - data input to send
// FIFO2 - transmit done token
//
// Data is sent in via FIFO1.
// They *very first* data transmitted on initialization is the mask that represents which I/O
// to drive the signal onto.
//
// Thereafter, the first piece of data sent in is the value for the last LED in the chain.
// Data has the following format:
// bit 24     - 0 means more data. 1 means transmit all previously sent values.
// bits 23:16 - g[7:0]
// bits 15:8  - r[7:0]
// bits 7:0   - b[7:0]
//
// Upon transmission, the buffer is cleared and the data is built up again from the last LED in the chain.
// Expects the quantum clock to be set to a 150ns period (6.66666..7 MHz)
//
// The transmit token is merely the current x31 register value (clock elapsed + core ID register)
//
// The three `li` instructions holding 0x400 are the pixel buffer address; they must
// match BUFFER_START above.
#[rustfmt::skip]
bio_code!(ws2812b_kernel, WS2812B_START, WS2812B_END,
    "mv x4, x17", // read from FIFO1 - the first argument is the GPIO pin mask we're using to transmit. stash this in x4
    "mv x26, x4", // apply mask to all GPIO operations
    // setup the pin as an output
    "mv x24, x4",
    // zero the output
    "mv x23, x0",

    // LEDs will go onto the stack
    "li x9, 0x1000000", // bit 24 mask
    "li sp, 0x400", // start of the LED buffer
"10:",
    // read a 24-bit color number from FIFO1
    "mv x8, x17",
    "sw x8, 0(sp)",
    "addi sp, sp, 4", // stack builds *UP*, away from the code
    "and x10, x9, x8", // AND incoming word with bit 24 mask
    "bne x10, x0, 20f", // jump to the routine at 20f to send if x10 is not 0
    "j 10b", // go back and get more data

    // -- sending loop --
"20:",
    "li x8, 0x400", // x8 now has the starting address of the data we're going to send
    // x9 is the bit 24 mask used by the above loop
    "li x12, 0x800000", // bit 23 mask
    // loop setup done
"30:",
    "li x11, 24", // number of bits to shift through
    "lw x10, 0(x8)", // fetch the word to send
"31:",
    "and x13, x12, x10", // the bit we're contemplating is at bit 23 position, extract it into x13
    "bne x13, x0, 40f", // if 1, go to 40f, where the 1 routine is

    "mv x20, x0", // snap to quantum before doing the send routine - this improves timing uniformity of high pulses
    // at the expense of T1L time being a little out of spec, but this is better-tolerated than T0H time being variable.
    // this quantum snap has to be introduced here because snap quantum to output makes the ws2812 driver not
    // composable with other BIO applications

    // zero routine
    // 2 hi
    "mv x22, x4", // sets to 1
    "mv x20, x0", // wait for quantum
    "mv x20, x0",
    // 7 lo
    "mv x23, x0", // sets to 0 because we set the GPIO mask earlier
    "mv x20, x0", // wait for quantum
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    // do work during the quantum - zero path
    "slli x10, x10, 1", // shift the pixel value
    "addi x11, x11, -1", // decrement the pixel bit counter
    "mv x20, x0",
    "j 50f", // jump to the loop end check

    // one routine
"40:",
    // 7 hi
    "mv x22, x4", // sets to 1
    "mv x20, x0", // wait for quantum
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    "mv x20, x0",
    // 2 lo
    "mv x23, x0", // sets to 0 because we set the GPIO mask earlier
    "mv x20, x0", // wait for quantum
    // do work during the quantum - one path
    "slli x10, x10, 1", // shift the pixel value
    "addi x11, x11, -1", // decrement the pixel bit counter
    "mv x20, x0",

"50:", // shift and do the next pixel value
    "bne x11, x0, 31b", // go back for more in the loop

"60:", // check if we've exhausted all the LED values
    "addi x8, x8, 4",
    "bge sp, x8, 30b", // see if we've hit the value of current sp
    "li sp, 0x400", // reset the stack pointer for a fresh fetch
    // wait to reset the chain
    "li x14, 2000", // delay wait time to reset the chain per spec
"70:",
    "addi x14, x14, -1",
    "mv x20, x0",
    "bne x14, x0, 70b",

    "mv x18, x31", // put a token in to synchronize and indicate that the loop is done

    "j 10b" // go back and get more data
);
