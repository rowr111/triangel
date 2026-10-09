use usb_bao1x::UsbHid;

use super::{EventSender, InputEvent};
use crate::setlist::SoundMode;

// On-screen previewer controls arrive as newline-terminated ASCII commands over USB
// serial (browser -> bridge.js -> here). The vocabulary mirrors the physical d-pad +
// 3-position switch in buttons.rs so the simulator behaves identically:
//   U/D = brightness up/down, L/R = pattern prev/next, C = toggle hold,
//   S0/S1/S2 = sound mode Off/Auto/On.

/// Spawn the previewer serial-input thread. Feeds the same event channel the physical
/// buttons do, so on-screen and real input are interchangeable.
pub fn spawn(events: EventSender) {
    std::thread::spawn(move || recv_loop(events));
}

fn recv_loop(events: EventSender) {
    let usb = UsbHid::new();
    loop {
        // Blocks until a '\n'-terminated command arrives over USB serial.
        let line = usb.serial_wait_ascii(Some('\n'));
        for cmd in line.split_whitespace() {
            if let Some(ev) = parse_command(cmd) {
                events.send(ev).ok();
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
