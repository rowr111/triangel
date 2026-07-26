use core::num::NonZeroU32;
use std::time::{Duration, Instant};

use arbitrary_int::{Number, u5};
use bao1x_api::bio::*;
use bao1x_api::bio_code;
use bao1x_api::bio_resources::*;
use bao1x_api::{IoSetup, IoxDir, IoxEnable};
use bao1x_hal::bio::{Bio, CoreCsr};
use bao1x_hal::ifram::IframRange;
use utralib::utra::bio_bdma;

// Generic BIO edge-interval capture. A dedicated BIO core halts on each RISING
// edge of the configured pin (ExternalPin quantum mode fires on rising edges
// only) and writes the elapsed time since the previous rising edge, in BIO
// clock ticks, into a ring buffer in IFRAM. The ring buffers seconds of
// edges, so capture stays intact even when the thread draining it is not
// scheduled for tens of milliseconds.

/// Ring capacity in intervals; must match the index mask in the BIO kernel.
const RING_N: usize = 256;
const RING_MASK: u32 = (RING_N - 1) as u32;
/// Ring layout in words: [0] = head (the BIO's free-running write counter),
/// [1] = startup magic, [2..2+RING_N] = intervals in BIO clock ticks.
const RING_WORDS: usize = 2 + RING_N;
/// Startup marker the kernel writes once it is running; reading it back
/// confirms the kernel can write the ring and the CPU can see the writes.
const MAGIC: u32 = 0x0B10ACED;

/// Cache-flush hint so the CPU re-reads memory the BIO wrote instead of a
/// stale cached copy.
#[inline(always)]
fn cache_flush() {
    // Safety: a hint instruction with no memory operands of its own.
    unsafe {
        core::arch::asm!(".word 0x500F", "nop", "nop", "nop", "nop", "nop");
    }
}

pub struct PulseCapture {
    bio_ss: Bio,
    // tracks the resources used by the object
    resource_grant: ResourceGrant,
    pin: u5,
    /// BIO clock frequency in Hz - the rate the interval timestamps tick at.
    clock_hz: u32,
    /// Shared IFRAM ring the BIO writes and we read.
    ring: IframRange,
    /// CPU read counter (free-running, like the head).
    tail: u32,
}

impl Resources for PulseCapture {
    fn resource_spec() -> ResourceSpec {
        ResourceSpec {
            claimer: "PulseCapture".to_string(),
            cores: vec![CoreRequirement::Any],
            // FIFO0 carries only the startup handshake; interval data flows
            // through the ring.
            fifos: vec![Fifo::Fifo0],
            static_pins: vec![],
            dynamic_pin_count: 1,
        }
    }
}

impl Drop for PulseCapture {
    fn drop(&mut self) {
        for &core in self.resource_grant.cores.iter() {
            self.bio_ss.de_init_core(core).unwrap();
        }
        self.bio_ss.release_dynamic_pin(self.pin.as_u8(), &Self::resource_spec().claimer).unwrap();
        self.bio_ss.release_resources(self.resource_grant.grant_id).unwrap();
    }
}

impl PulseCapture {
    /// `probe_out` receives startup diagnostic lines; pass a no-op closure if
    /// they are not wanted.
    pub fn new(pin: u5, probe_out: &dyn Fn(&str)) -> Result<Self, BioError> {
        // Electrical setup: schmitt-trigger input with the internal pull-up on,
        // suiting an idle-high open-collector style signal like an IR receiver.
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

        // Allocate the shared ring in IFRAM Bank 0. BIO stores do not reach
        // Bank 1, so the ring must stay in Bank 0.
        // Safety: the IframRange lives in this program-lifetime object.
        let ring_bytes = RING_WORDS * 4;
        let ring = unsafe { IframRange::request(ring_bytes, None) }.ok_or(BioError::Oom)?;
        let ring_phys = ring.phys_range.as_ptr() as u32;

        let mut bio_ss = Bio::new();
        // claim core resource and initialize it
        let resource_grant = bio_ss.claim_resources(&Self::resource_spec())?;

        // Allow the BIO to write the ring's memory. Window base and bounds
        // are in pages.
        let base_page = ring_phys >> 12;
        let end_page = (ring_phys + ring_bytes as u32 + 0xFFF) >> 12;
        let window = DmaWindow {
            base: base_page,
            bounds: NonZeroU32::new(end_page - base_page).expect("ring spans at least one page"),
        };
        bio_ss.setup_dma_windows(DmaFilterWindows { windows: [Some(window), None, None, None] })?;

        let config =
            CoreConfig { clock_mode: bao1x_api::bio::ClockMode::ExternalPin(BioPin::new(pin.as_u8())) };
        bio_ss.init_core(resource_grant.cores[0], pulse_capture_code(), config)?;

        // claim pin resource - this only claims the resource, it does not configure it
        bio_ss.claim_dynamic_pin(pin.as_u8(), &Self::resource_spec().claimer)?;
        // now configure the claimed resource. SetOnly maps this pin without
        // disturbing pins other drivers have mapped.
        let io_config = IoConfig {
            mapped: 1 << pin.as_u32(),
            mode: IoConfigMode::SetOnly,
            ..Default::default()
        };
        bio_ss.setup_io_config(io_config).unwrap();

        bio_ss.set_core_run_state(&resource_grant, true);

        let clock_hz = bio_ss.get_bio_freq();

        // Hand the kernel its pin mask, then the ring's physical base, over
        // FIFO0. The kernel writes the probe values and blocks until the host
        // sends a third "go" word. Scoped so the handle drops after the
        // handshake - the BIO never uses FIFO0 again.
        {
            let fifo_handle =
                unsafe { bio_ss.get_core_handle(Fifo::Fifo0) }?.expect("Didn't get FIFO0 handle");
            let mut tx = CoreCsr::from_handle(&fifo_handle);
            tx.csr.wo(bio_bdma::SFR_TXF0, 1 << pin.as_u32());
            tx.csr.wo(bio_bdma::SFR_TXF0, ring_phys);

            // Read back and report the kernel's probe writes.
            std::thread::sleep(Duration::from_millis(50));
            cache_flush();
            let mut probe = [0u32; 4];
            for (i, slot) in probe.iter_mut().enumerate() {
                // Safety: virt_range maps the ring; words 0-3 are the probe area.
                *slot =
                    unsafe { (ring.virt_range.as_ptr() as *const u32).add(i).read_volatile() };
            }
            probe_out(&format!(
                "ring store probe: w0 {:08x} w1 {:08x} w2 {:08x} w3 {:08x} (sent aaaa0000 bbbb0004 cccc0008 dddd000c)",
                probe[0], probe[1], probe[2], probe[3]
            ));

            tx.csr.wo(bio_bdma::SFR_TXF0, 1); // go: kernel clears head, writes magic, starts capture
        }

        // Wait briefly for the kernel's magic; report and continue if it is
        // slow to appear.
        let start = Instant::now();
        loop {
            cache_flush();
            // Safety: virt_range maps the ring; word 1 is the magic.
            let magic =
                unsafe { (ring.virt_range.as_ptr() as *const u32).add(1).read_volatile() };
            if magic == MAGIC {
                break;
            }
            if start.elapsed() > Duration::from_millis(500) {
                probe_out("ring magic not seen after go; continuing for diagnosis");
                log::error!("PulseCapture: BIO never wrote its startup magic to the ring");
                break;
            }
        }

        Ok(Self { bio_ss, resource_grant, pin, clock_hz, ring, tail: 0 })
    }

    /// BIO clock frequency the intervals are measured in.
    pub fn clock_hz(&self) -> u32 { self.clock_hz }

    /// Read the BIO's write counter, flushing the cache first so we see its
    /// writes rather than a stale cached copy.
    fn head(&self) -> u32 {
        cache_flush();
        // Safety: virt_range maps the ring; word 0 is the head.
        unsafe { (self.ring.virt_range.as_ptr() as *const u32).read_volatile() }
    }

    /// Interval between the two most recent rising edges in raw BIO clock
    /// ticks, or None if no unread interval is in the ring. The first interval
    /// after startup measures from init to the first edge and should be
    /// ignored by callers.
    pub fn try_read_ticks(&mut self) -> Option<u32> {
        let head = self.head();
        if head == self.tail {
            return None;
        }
        // Overrun: if the BIO lapped us, resync to the oldest intact entry.
        if head.wrapping_sub(self.tail) > RING_N as u32 {
            self.tail = head.wrapping_sub(RING_N as u32);
        }
        let off = 2 + (self.tail & RING_MASK) as usize; // words 0/1 are head/magic
        // Safety: off is within the mapped ring (masked to RING_N, +2 for header).
        let ticks =
            unsafe { (self.ring.virt_range.as_ptr() as *const u32).add(off).read_volatile() };
        self.tail = self.tail.wrapping_add(1);
        Some(ticks)
    }

    /// Same as try_read_ticks, converted to microseconds.
    pub fn try_read_us(&mut self) -> Option<u32> {
        self.try_read_ticks().map(|ticks| (ticks as u64 * 1_000_000 / self.clock_hz as u64) as u32)
    }

    /// Diagnostic snapshot: (head, tail, magic word as currently read).
    pub fn debug_state(&self) -> (u32, u32, u32) {
        let head = self.head();
        // Safety: virt_range maps the ring; word 1 is the magic.
        let magic =
            unsafe { (self.ring.virt_range.as_ptr() as *const u32).add(1).read_volatile() };
        (head, self.tail, magic)
    }
}

#[rustfmt::skip]
bio_code!(
    pulse_capture_code,
    PULSE_CAPTURE_START,
    PULSE_CAPTURE_END,

    "mv    x5, x16",         // pin bitmask from FIFO0 (blocks until the host sends it)
    "mv    x26, x5",         // set GPIO mask to our pin
    "mv    x25, x5",         // configure pin as an input
    "mv    x6, x16",         // ring physical base from FIFO0 (second pop)

    // startup probe: four known values the host reads back and reports
    "li    x4, 0xAAAA0000",
    "sw    x4, 0(x6)",
    "li    x4, 0xBBBB0004",
    "sw    x4, 4(x6)",
    "li    x4, 0xCCCC0008",
    "sw    x4, 8(x6)",
    "li    x4, 0xDDDD000C",
    "sw    x4, 12(x6)",

    "mv    x4, x16",         // block until the host has read the probe (third pop)

    // ring layout: 0(x6) = head, 4(x6) = magic, 8(x6).. = interval entries
    "sw    x0, 0(x6)",       // head = 0
    "li    x4, 0x0B10ACED",  // startup magic - must match MAGIC on the host
    "sw    x4, 4(x6)",
    "li    x11, 0",          // head counter
    "li    x12, 0xFF",       // ring index mask - must match RING_N - 1 on the host

    "mv    x7, x31",         // seed the "previous edge" timestamp
"20:", // one loop iteration per rising edge
    "mv    x20, x0",         // wait for quantum = rising edge on the configured pin
    "mv    x8, x31",         // timestamp this edge

    // mask out core ID bits from both timestamps
    "li    x10, 0x3FFFFFFF", // x10 is re-used in the roll-over computation below
    "and   x7, x7, x10",
    "and   x8, x8, x10",

    // handle roll-over case: x7 is greater than x8 in the case of roll-over
    "bgtu  x7, x8, 30f",
    "sub   x9, x8, x7",      // interval is x8 - x7
    "j     40f",
"30:", // roll-over path
    "sub   x10, x10, x7",    // x10 now contains ticks from start to the max count
    "add   x9, x10, x8",     // interval is x10 + x8 (masked)
"40:",
    // entry store first, then the head publish, so a reader that sees the new
    // head is guaranteed to see the entry
    "and   x13, x11, x12",   // slot = head & mask
    "slli  x13, x13, 2",     // slot * 4 bytes
    "add   x13, x13, x6",
    "sw    x9, 8(x13)",      // entries start at byte offset 8
    "addi  x11, x11, 1",
    "sw    x11, 0(x6)",      // publish head
    "mv    x7, x8",          // this edge becomes the previous edge
    "j     20b"
);
