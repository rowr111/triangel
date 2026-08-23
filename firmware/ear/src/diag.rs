//! USB-serial console for mic bringup. The ear's `log::info!` output is not
//! reachable from a serial monitor - UART2 carries the audio link to the eye -
//! so this is the only visibility into the firmware.

use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

use usb_bao1x::UsbHid;

/// Startup milestones, so a heartbeat line names where a stall happened.
pub const STAGES: &[&str] =
    &["usb up", "hal up", "starting BIO core", "BIO core up", "prompt ready"];
static STAGE: AtomicUsize = AtomicUsize::new(0);
static QUIET: AtomicBool = AtomicBool::new(false);
/// Bytes ever received from the host, reported in the heartbeat so a dead input
/// path is distinguishable from one that arrives but does not parse.
static RX_BYTES: AtomicU32 = AtomicU32::new(0);

/// Pause after each line. Sent back to back, bulk output arrives at the host with
/// lines duplicated and others missing entirely, so it has to be paced.
const LINE_PACE_MS: usize = 5;

pub struct Diag {
    usb: UsbHid,
    tt: ticktimer::Ticktimer,
}

impl Diag {
    pub fn new() -> Self {
        Self { usb: UsbHid::new(), tt: ticktimer::Ticktimer::new().unwrap() }
    }

    /// Send one line. Text and line ending go in a single send so lines from
    /// different threads cannot interleave mid-line.
    pub fn line(&self, s: &str) {
        let mut out = String::with_capacity(s.len() + 2);
        out.push_str(s);
        out.push_str("\r\n");
        self.usb.serial_send(out.as_bytes()).ok();
        self.tt.sleep_ms(LINE_PACE_MS).ok();
    }

    /// Print the prompt and block until a command character arrives. Commands are
    /// a single character and act on the keystroke: terminals disagree on whether
    /// Enter sends CR, LF, or both, so requiring a terminator only invites the
    /// prompt to hang. CR and LF are skipped rather than treated as commands.
    pub fn command(&self) -> char {
        self.usb.serial_send(b"\r\nmic> ").ok();
        loop {
            for b in self.usb.serial_wait_binary() {
                RX_BYTES.fetch_add(1, Ordering::Relaxed);
                if b.is_ascii_graphic() {
                    // Echo it back - the monitor runs with local echo off.
                    self.usb.serial_send(&[b, b'\r', b'\n']).ok();
                    return b as char;
                }
            }
        }
    }
}

/// Record a startup stage and print it.
pub fn stage(d: &Diag, idx: usize) {
    STAGE.store(idx, Ordering::Relaxed);
    d.line(&format!("stage: {}", STAGES[idx]));
}

/// Stop the heartbeat, so command output is not broken up by it.
pub fn quiet() {
    QUIET.store(true, Ordering::Relaxed);
}

/// Print a liveness line every 2 s until the first command arrives. Spawned before
/// anything that can block, so a stalled startup still names the stage it stopped
/// at - and so a monitor attached after boot sees something without having to type.
pub fn spawn_heartbeat() {
    std::thread::spawn(|| {
        let tt = ticktimer::Ticktimer::new().unwrap();
        let d = Diag::new();
        loop {
            tt.sleep_ms(2000).ok();
            if QUIET.load(Ordering::Relaxed) {
                return;
            }
            d.line(&format!(
                "ear alive {} s, stage: {}, rx {} bytes",
                tt.elapsed_ms() / 1000,
                STAGES[STAGE.load(Ordering::Relaxed)],
                RX_BYTES.load(Ordering::Relaxed)
            ));
        }
    });
}
