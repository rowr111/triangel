use std::sync::atomic::{AtomicUsize, Ordering};

// USB-serial diagnostic channel for bringup on hardware. The console input
// path is unavailable on this system build (UART2 is repurposed for the ear
// audio link), so these unconditionally streamed lines are the only
// visibility into the firmware. Active only on builds with `usb` and without
// `previewer` - the previewer's serial stream carries binary LED frames and
// text mixed in would corrupt it. On all other builds every call compiles to
// a no-op.

/// Boot milestones, printed as reached and repeated in the heartbeat line so
/// a monitor attached late can still see how far boot got.
pub const STAGES: &[&str] =
    &["main start", "log up", "led out up", "audio up", "input spawned", "render loop"];
static STAGE: AtomicUsize = AtomicUsize::new(0);

pub struct Diag {
    #[cfg(all(feature = "usb", not(feature = "previewer")))]
    usb: usb_bao1x::UsbHid,
}

impl Diag {
    pub fn new() -> Self {
        Self {
            #[cfg(all(feature = "usb", not(feature = "previewer")))]
            usb: usb_bao1x::UsbHid::new(),
        }
    }

    /// Send one diagnostic line to the USB serial port (no-op on builds
    /// without it). One send per line: text and line ending in separate sends
    /// let lines from different threads interleave mid-line.
    pub fn line(&self, s: &str) {
        #[cfg(all(feature = "usb", not(feature = "previewer")))]
        {
            let mut out = String::with_capacity(s.len() + 2);
            out.push_str(s);
            out.push_str("\r\n");
            self.usb.serial_send(out.as_bytes()).ok();
        }
        #[cfg(not(all(feature = "usb", not(feature = "previewer"))))]
        let _ = s;
    }
}

/// Record a boot stage (index into STAGES) and print it.
pub fn stage(diag: &Diag, idx: usize) {
    STAGE.store(idx, Ordering::Relaxed);
    diag.line(&format!("eye boot: {}", STAGES[idx]));
}

/// Report liveness and IR activity: one line shortly after boot, then a line
/// at most every 5 seconds and only when the IR counters have changed - the
/// steady-state serial output is silent. Spawned before anything that could
/// block in main so a stalled boot still heartbeats and names the stage it
/// is stuck at.
pub fn spawn_heartbeat() {
    #[cfg(all(feature = "usb", not(feature = "previewer")))]
    std::thread::spawn(|| {
        let tt = ticktimer::Ticktimer::new().unwrap();
        let diag = Diag::new();
        diag.line("heartbeat thread up");
        let mut last_stats = None;
        loop {
            tt.sleep_ms(5000).ok();
            let stats = crate::input::ir::stats();
            if last_stats == Some(stats) {
                continue;
            }
            last_stats = Some(stats);
            let (clock_hz, decoded, rejected, last_frame) = stats;
            diag.line(&format!(
                "alive {} s, stage: {}, ir: decoded {}, rejected {}, clock {} Hz, last frame {:08x}",
                tt.elapsed_ms() / 1000,
                STAGES[STAGE.load(Ordering::Relaxed)],
                decoded,
                rejected,
                clock_hz,
                last_frame,
            ));
        }
    });
}
