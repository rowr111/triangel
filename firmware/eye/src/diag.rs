use std::sync::Mutex;

// Status lines over USB serial, the only view into the firmware: UART2, the usual
// console, carries the ear's audio link.

/// True on builds that print these lines. The previewer's serial stream carries LED
/// frames, which text would corrupt.
pub const ON: bool = cfg!(all(feature = "bringup", not(feature = "previewer")));

/// Boot milestones, printed as reached and repeated by the heartbeat until boot is done.
#[derive(Clone, Copy)]
pub enum Stage {
    MainStart,
    LedOutUp,
    AudioUp,
    InputSpawned,
    RenderLoop,
}

impl Stage {
    fn name(self) -> &'static str {
        match self {
            Stage::MainStart => "main start",
            Stage::LedOutUp => "led out up",
            Stage::AudioUp => "audio up",
            Stage::InputSpawned => "input spawned",
            Stage::RenderLoop => "render loop",
        }
    }
}

static STAGE: Mutex<Stage> = Mutex::new(Stage::MainStart);

pub struct Diag {
    #[cfg(all(feature = "bringup", not(feature = "previewer")))]
    usb: usb_bao1x::UsbHid,
}

impl Diag {
    pub fn new() -> Self {
        Self {
            #[cfg(all(feature = "bringup", not(feature = "previewer")))]
            usb: usb_bao1x::UsbHid::new(),
        }
    }

    /// Send one line. One send per line, or lines from different threads could
    /// interleave mid-line.
    pub fn line(&self, s: &str) {
        #[cfg(all(feature = "bringup", not(feature = "previewer")))]
        {
            let mut out = String::with_capacity(s.len() + 2);
            out.push_str(s);
            out.push_str("\r\n");
            self.usb.serial_send(out.as_bytes()).ok();
        }
        let _ = s;
    }
}

/// Record a boot stage and print it.
pub fn stage(diag: &Diag, stage: Stage) {
    *STAGE.lock().unwrap() = stage;
    diag.line(&format!("eye boot: {}", stage.name()));
}

/// Every 5 seconds: the audio link and drop lines when the link has changed, and the
/// alive line until boot is done, then only when the IR counters change. Spawned before
/// anything in main that could block, so a stalled boot keeps naming its stage.
pub fn spawn_heartbeat() {
    if !ON {
        return;
    }
    std::thread::spawn(|| {
        let tt = ticktimer::Ticktimer::new().unwrap();
        let diag = Diag::new();
        diag.line("heartbeat thread up");
        let mut last_stats = None;
        let mut last_audio = None;
        loop {
            tt.sleep_ms(5000).ok();

            // Audio link: counters first, so bytes-but-no-frames is distinguishable
            // from nothing arriving at all.
            let link = crate::audio::stats();
            let seen = (link.status, link.first_byte, link.frames_ok, link.frames_bad);
            if last_audio != Some(seen) {
                last_audio = Some(seen);
                diag.line(&format!(
                    "audio: status {}, frames {}, dropped {}, first byte {:02x}, {:.1} dBFS, norm {:.2}",
                    link.status, link.frames_ok, link.frames_bad, link.first_byte, link.dbfs, link.norm,
                ));
                let bass = crate::audio::drop_stats();
                diag.line(&format!(
                    "drop: {}, drops {}, bass {:.1} dB against {:.1} dB",
                    if bass.in_breakdown { "in breakdown" } else { "normal" },
                    bass.drops, bass.bass_db, bass.ref_db,
                ));
            }

            let ir = crate::input::ir::stats();
            let stage = *STAGE.lock().unwrap();
            if matches!(stage, Stage::RenderLoop) && last_stats == Some(ir) {
                continue;
            }
            last_stats = Some(ir);
            diag.line(&format!(
                "alive {} s, stage: {}, ir: decoded {}, rejected {}, clock {} Hz, last frame {:08x}",
                tt.elapsed_ms() / 1000,
                stage.name(),
                ir.decoded,
                ir.rejected,
                ir.clock_hz,
                ir.last_frame,
            ));
        }
    });
}
