use arbitrary_int::{Number, u5};
use bao1x_api::bio::*;
use bao1x_api::bio_code;
use bao1x_api::bio_resources::*;
use bao1x_hal::bio::{Bio, CoreCsr};
use utralib::utra::bio_bdma;

use crate::led::map::{CHAIN1_LED_COUNT, CHAIN2_LED_COUNT};

// Two WS2812B chains on two BIO cores, clocked out in parallel so a frame costs
// the longer chain's ~9.4ms rather than the sum of both.
//
// Each chain gets one FIFO for pixel data - chain 1 on Fifo1, chain 2 on Fifo2 -
// and neither needs a second FIFO for the return trip: a core raises its own bit
// in the shared event register once the strip is clocked out, and the host polls
// and clears those bits. That leaves Fifo0 to the IR receiver and Fifo3 unclaimed.
//
// The kernels below are bio-lib's `ws2812b_kernel` with three changes: the FIFO
// the core reads from, the completion signal, and the pixel buffer address.

/// Event register bits the two cores raise when a strip has finished sending.
/// Bits 0-23 are free for software use; 24-31 are hard-wired FIFO level flags.
const CHAIN1_DONE: u32 = 1 << 0;
const CHAIN2_DONE: u32 = 1 << 1;
const BOTH_DONE:   u32 = CHAIN1_DONE | CHAIN2_DONE;

/// Byte offset in a core's memory where its pixel buffer starts. Must match the
/// three `li` instructions in each kernel below. bio-lib puts this at 0x800,
/// which caps a chain at 512 LEDs; the kernel is only ~240 bytes, so starting
/// lower costs nothing and leaves room to grow a chain.
const BUFFER_START: usize = 0x400;
/// A BIO core's total memory, kernel code and pixel buffer together.
const CORE_MEM_BYTES: usize = 4096;
/// Longest chain a core's buffer can hold, at 4 bytes per LED.
pub const MAX_LEDS: usize = (CORE_MEM_BYTES - BUFFER_START) / 4;

const LONGEST_CHAIN: usize =
    if CHAIN1_LED_COUNT > CHAIN2_LED_COUNT { CHAIN1_LED_COUNT } else { CHAIN2_LED_COUNT };
const _: () = assert!(
    LONGEST_CHAIN <= MAX_LEDS,
    "a chain exceeds the BIO core's pixel buffer; lower BUFFER_START (and the kernels' li instructions) to fit"
);

/// 150ns quantum - the WS2812B bit timings in the kernels are counted in these.
const QUANTUM_HZ: u32 = 6_666_667;

pub struct Ws2812Pair {
    bio_ss: Bio,
    pin1: u5,
    pin2: u5,
    // handles have to be kept around or else the underlying CSR is dropped
    _fifo1_handle: CoreHandle,
    _fifo2_handle: CoreHandle,
    fifo1: CoreCsr,
    fifo2: CoreCsr,
    resource_grant: ResourceGrant,
}

impl Resources for Ws2812Pair {
    fn resource_spec() -> ResourceSpec {
        ResourceSpec {
            claimer: "Ws2812Pair".to_string(),
            cores: vec![CoreRequirement::Any, CoreRequirement::Any],
            fifos: vec![Fifo::Fifo1, Fifo::Fifo2],
            static_pins: vec![],
            dynamic_pin_count: 2,
        }
    }
}

impl Drop for Ws2812Pair {
    fn drop(&mut self) {
        for &core in self.resource_grant.cores.iter() {
            self.bio_ss.de_init_core(core).unwrap();
        }
        let claimer = Ws2812Pair::resource_spec().claimer;
        self.bio_ss.release_dynamic_pin(self.pin1.as_u8(), &claimer).unwrap();
        self.bio_ss.release_dynamic_pin(self.pin2.as_u8(), &claimer).unwrap();
        self.bio_ss.release_resources(self.resource_grant.grant_id).unwrap();
    }
}

impl Ws2812Pair {
    /// Claims two cores and two FIFOs as one unit: either both chains come up or
    /// neither does, so a half-lit panel can never be the steady state.
    pub fn new(pin1: u5, pin2: u5) -> Result<Self, BioError> {
        let mut bio_ss = Bio::new();
        let resource_grant = bio_ss.claim_resources(&Self::resource_spec())?;
        let config = CoreConfig { clock_mode: ClockMode::TargetFreqInt(QUANTUM_HZ) };
        bio_ss.init_core(resource_grant.cores[0], ws2812b_fifo1_kernel(), config)?;
        bio_ss.init_core(resource_grant.cores[1], ws2812b_fifo2_kernel(), config)?;
        bio_ss.set_core_run_state(&resource_grant, true);

        let claimer = Self::resource_spec().claimer;
        bio_ss.claim_dynamic_pin(pin1.as_u8(), &claimer)?;
        bio_ss.claim_dynamic_pin(pin2.as_u8(), &claimer)?;
        // SetOnly leaves the pins other drivers have mapped alone; Overwrite would
        // unmap the IR receiver's pin, and each chain would unmap the other's.
        let io_config = IoConfig {
            mapped: (1 << pin1.as_u32()) | (1 << pin2.as_u32()),
            mode: IoConfigMode::SetOnly,
            ..Default::default()
        };
        bio_ss.setup_io_config(io_config).unwrap();

        // safety: the handles are stored in the struct so they outlive the CoreCsr views
        let fifo1_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo1) }?.expect("Didn't get FIFO1 handle");
        let fifo2_handle = unsafe { bio_ss.get_core_handle(Fifo::Fifo2) }?.expect("Didn't get FIFO2 handle");
        let mut fifo1 = CoreCsr::from_handle(&fifo1_handle);
        let mut fifo2 = CoreCsr::from_handle(&fifo2_handle);

        // Startup words, once per core: the pin mask to drive, then the event bit to
        // raise on completion. Passing the bit as data keeps the two kernels identical
        // apart from their FIFO register.
        fifo1.csr.wo(bio_bdma::SFR_TXF1, 1 << pin1.as_u32());
        fifo1.csr.wo(bio_bdma::SFR_TXF1, CHAIN1_DONE);
        fifo2.csr.wo(bio_bdma::SFR_TXF2, 1 << pin2.as_u32());
        fifo2.csr.wo(bio_bdma::SFR_TXF2, CHAIN2_DONE);

        // Any bits left over from a previous run would read as an instant completion.
        fifo1.csr.wo(bio_bdma::SFR_EVENT_CLR, BOTH_DONE);

        Ok(Self {
            bio_ss,
            pin1,
            pin2,
            _fifo1_handle: fifo1_handle,
            _fifo2_handle: fifo2_handle,
            fifo1,
            fifo2,
            resource_grant,
        })
    }

    /// Hands both strips to their cores and returns without waiting. Bit 24 of the
    /// last word tells a core the strip is complete and transmission may start, so
    /// chain 1 is already sending while chain 2 is still being loaded.
    pub fn send_async(&mut self, chain1: &[u32], chain2: &[u32]) {
        // Clear first, before either core can raise its bit: leaving the previous
        // frame's flags up would make the next `send_await` return immediately.
        self.fifo1.csr.wo(bio_bdma::SFR_EVENT_CLR, BOTH_DONE);
        if let Some((&last, elements)) = chain1.split_last() {
            for &led in elements.iter() {
                self.fifo1.csr.wo(bio_bdma::SFR_TXF1, led);
            }
            self.fifo1.csr.wo(bio_bdma::SFR_TXF1, last | 0x1_00_00_00);
        }
        if let Some((&last, elements)) = chain2.split_last() {
            for &led in elements.iter() {
                self.fifo2.csr.wo(bio_bdma::SFR_TXF2, led);
            }
            self.fifo2.csr.wo(bio_bdma::SFR_TXF2, last | 0x1_00_00_00);
        }
    }

    /// Waits for both strips to finish. Skipping this before the next `send_async`
    /// risks overflowing a FIFO while its core is busy.
    pub fn send_await(&self) {
        // Both FIFOs live in one register block, so either CSR view reaches the
        // shared event register.
        while self.fifo1.csr.r(bio_bdma::SFR_EVENT_STATUS) & BOTH_DONE != BOTH_DONE {
            xous::yield_slice();
        }
    }
}

/// Packs RGB into the GRB word order the WS2812 expects.
pub fn rgb_to_u32(r: u8, g: u8, b: u8) -> u32 { ((g as u32) << 16) | ((r as u32) << 8) | (b as u32) }

// WS2812B kernel, chain 1 - reads pixel data from FIFO1 (x17).
//
// Startup words, in order:
//   1. the I/O mask naming the pin to drive
//   2. the event register bit to raise when a strip has been sent
//
// Every word after that is one LED, from the far end of the chain back:
//   bit 24     - 0 means more data follows. 1 means send everything received.
//   bits 23:16 - g[7:0]
//   bits 15:8  - r[7:0]
//   bits 7:0   - b[7:0]
//
// After sending, the buffer is cleared and rebuilt from the last LED again.
// Expects the quantum clock to be set to a 150ns period (6.66666..7 MHz).
//
// The three `li` instructions holding 0x400 are the pixel buffer address; they
// must match BUFFER_START above.
#[rustfmt::skip]
bio_code!(ws2812b_fifo1_kernel, WS2812B_FIFO1_START, WS2812B_FIFO1_END,
    "mv x4, x17", // read from FIFO1 - the first argument is the GPIO pin mask we're using to transmit. stash this in x4
    "mv x15, x17", // second argument: the event bit this core raises when a strip is done
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

    "mv x28, x15", // raise this core's event bit to tell the host the strip is sent

    "j 10b" // go back and get more data
);

// WS2812B kernel, chain 2 - identical to chain 1 except it reads pixel data
// from FIFO2 (x18).
#[rustfmt::skip]
bio_code!(ws2812b_fifo2_kernel, WS2812B_FIFO2_START, WS2812B_FIFO2_END,
    "mv x4, x18", // read from FIFO2 - the first argument is the GPIO pin mask we're using to transmit. stash this in x4
    "mv x15, x18", // second argument: the event bit this core raises when a strip is done
    "mv x26, x4", // apply mask to all GPIO operations
    // setup the pin as an output
    "mv x24, x4",
    // zero the output
    "mv x23, x0",

    // LEDs will go onto the stack
    "li x9, 0x1000000", // bit 24 mask
    "li sp, 0x400", // start of the LED buffer
"10:",
    // read a 24-bit color number from FIFO2
    "mv x8, x18",
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

    "mv x28, x15", // raise this core's event bit to tell the host the strip is sent

    "j 10b" // go back and get more data
);
