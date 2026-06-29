use usb_bao1x::UsbHid;

use super::{EventQueue, InputEvent};
use crate::setlist::SoundMode;

// On-screen previewer controls arrive as newline-terminated ASCII commands over USB
// serial (browser -> bridge.js -> here). The vocabulary mirrors the physical d-pad +
// 3-position switch in buttons.rs so the simulator behaves identically:
//   U/D = brightness up/down, L/R = pattern prev/next, C = toggle hold,
//   S0/S1/S2 = sound mode Off/Auto/On.

/// Spawn the previewer serial-input thread. Feeds the same event queue the physical
/// buttons do, so on-screen and real input are interchangeable.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || recv_loop(queue));
}

fn recv_loop(queue: EventQueue) {
    let usb = UsbHid::new();
    loop {
        // Blocks until a '\n'-terminated command arrives over USB serial.
        let line = usb.serial_wait_ascii(Some('\n'));
        for cmd in line.split_whitespace() {
            if let Some(ev) = parse_command(cmd) {
                if let Ok(mut q) = queue.lock() {
                    q.push_back(ev);
                }
            }
        }
    }
}

fn parse_command(cmd: &str) -> Option<InputEvent> {
    match cmd {
        "U"  => Some(InputEvent::BrightnessUp),
        "D"  => Some(InputEvent::BrightnessDown),
        "L"  => Some(InputEvent::PatternPrev),
        "R"  => Some(InputEvent::PatternNext),
        "C"  => Some(InputEvent::ToggleHold),
        "S0" => Some(InputEvent::SetSoundMode(SoundMode::Off)),
        "S1" => Some(InputEvent::SetSoundMode(SoundMode::Auto)),
        "S2" => Some(InputEvent::SetSoundMode(SoundMode::On)),
        _    => None,
    }
}
