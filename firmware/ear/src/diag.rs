//! Output over USB serial: lines, startup stages and the heartbeat. UART2 carries the
//! audio link to the eye, so this is the only visibility into the firmware.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;

use usb_bao1x::UsbHid;

/// Startup milestones, so a heartbeat line names where a stall happened.
#[derive(Clone, Copy)]
pub enum Stage {
    UsbUp,
    HalUp,
    StartingBio,
    BioUp,
    PromptReady,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::UsbUp => "usb up",
            Stage::HalUp => "hal up",
            Stage::StartingBio => "starting BIO core",
            Stage::BioUp => "BIO core up",
            Stage::PromptReady => "prompt ready",
        }
    }
}

static STAGE: Mutex<Stage> = Mutex::new(Stage::UsbUp);
static QUIET: AtomicBool = AtomicBool::new(false);
/// Bytes received from the host, shown in the heartbeat to tell a dead input path from
/// bytes that are not keys.
static RX_BYTES: AtomicU32 = AtomicU32::new(0);

/// Pause after each line. Sent back to back, bulk output arrives at the host with
/// lines duplicated and others missing entirely, so it has to be paced.
const LINE_PACE_MS: usize = 5;

const HEARTBEAT_MS: usize = 2_000;

pub struct Diag {
    usb: UsbHid,
    tt: ticktimer::Ticktimer,
}

impl Diag {
    pub fn new() -> Self {
        Self {
            usb: UsbHid::new(),
            tt: ticktimer::Ticktimer::new().unwrap(),
        }
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

    /// Print the prompt and block until a printable key arrives. Commands need no Enter,
    /// since terminals disagree on what Enter sends.
    pub fn command(&self) -> char {
        self.usb.serial_send(b"\r\nmic> ").ok();
        loop {
            let batch = self.usb.serial_wait_binary();
            RX_BYTES.fetch_add(batch.len() as u32, Ordering::Relaxed);
            // The newest key wins; anything typed before it is dropped.
            if let Some(&b) = batch.iter().rev().find(|b| b.is_ascii_graphic()) {
                // Echo it back - the monitor runs with local echo off.
                self.usb.serial_send(&[b, b'\r', b'\n']).ok();
                return b as char;
            }
        }
    }
}

/// Record a startup stage and print it.
pub fn stage(d: &Diag, stage: Stage) {
    *STAGE.lock().unwrap() = stage;
    d.line(&format!("stage: {}", stage.name()));
}

/// Stop the heartbeat, so command output is not broken up by it.
pub fn quiet() {
    QUIET.store(true, Ordering::Relaxed);
}

/// Print a liveness line naming the current stage every HEARTBEAT_MS until the first
/// command arrives, so a monitor attached after boot sees something without having to type.
pub fn spawn_heartbeat() {
    std::thread::spawn(|| {
        let tt = ticktimer::Ticktimer::new().unwrap();
        let d = Diag::new();
        loop {
            tt.sleep_ms(HEARTBEAT_MS).ok();
            if QUIET.load(Ordering::Relaxed) {
                return;
            }
            let stage = *STAGE.lock().unwrap();
            d.line(&format!(
                "ear alive {} s, stage: {}, rx {} bytes",
                tt.elapsed_ms() / 1000,
                stage.name(),
                RX_BYTES.load(Ordering::Relaxed)
            ));
        }
    });
}
