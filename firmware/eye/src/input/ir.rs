use super::{EventQueue, InputEvent};
use crate::pins;

// IR receiver module data output: idle = HIGH, burst = LOW
// (module inverts and demodulates the 38kHz carrier).

// NEC protocol timing in microseconds.
const NEC_LEADER_PULSE_US: u32 = 9_000;
const NEC_LEADER_SPACE_US: u32 = 4_500;
const NEC_BIT_PULSE_US:    u32 =   560;
const NEC_BIT_0_SPACE_US:  u32 =   560;
const NEC_BIT_1_SPACE_US:  u32 = 1_690;
const NEC_REPEAT_SPACE_US: u32 = 2_250;
const NEC_TIMING_MARGIN:   u32 =   200; // +/-us tolerance

// Remote: 7-button remote. Buttons: up, down, left, right,
// center, gear, TV.
//
// Intended mapping:
//   Up     -> BrightnessUp
//   Down   -> BrightnessDown
//   Left   -> PatternPrev
//   Right  -> PatternNext
//   Center -> ToggleHold
//   Gear   -> CycleSoundMode
//   TV     -> (spare - TBD)
//
// Sound mode is handled by the physical 3-position switch, not IR.
// NEC address confirmed: usercode 00FF.
const NEC_ADDR: u8 = 0x00;

const IR_CMD_BRIGHTNESS_UP:   u8 = 0x2B; // Up button
const IR_CMD_BRIGHTNESS_DOWN: u8 = 0x2C; // Down button
const IR_CMD_PATTERN_NEXT:    u8 = 0x29; // Right button
const IR_CMD_PATTERN_PREV:    u8 = 0x2A; // Left button
const IR_CMD_HOLD:            u8 = 0x28; // Center button
const IR_CMD_GEAR:            u8 = 0x2E; // Gear button -> cycle sound mode
#[allow(dead_code)]
const IR_CMD_TV:              u8 = 0x2D; // TV button - spare (use TBD)

/// Spawn the IR remote receiver thread.
pub fn spawn(queue: EventQueue) {
    std::thread::spawn(move || {
        poll_loop(queue);
    });
}

fn poll_loop(queue: EventQueue) {
    let tt = ticktimer::Ticktimer::new().unwrap();
    loop {
        // TODO: implement NEC IR frame decode.
        //
        // NEC frame structure (as seen after the receiver module):
        //   9ms LOW (leader burst) + 4.5ms HIGH (leader space)
        //   + 32 bits: each bit = 560us LOW pulse + space
        //     (560us space = 0, 1690us space = 1)
        //   + final 560us LOW pulse
        //   + idle HIGH
        //
        // 32 bits = address (8) + ~address (8) + command (8) + ~command (8).
        // Validate: address ^ ~address == 0xFF and cmd ^ ~cmd == 0xFF.
        //
        // Options for timing on bao1x:
        //   a) BIO core - configure as pulse-width capture (most efficient, no CPU spin)
        //   b) GPIO interrupt + timestamp - OS-supported, reasonable resolution
        //   c) Busy-wait at ~100us intervals - simple but spins CPU for 67ms per frame
        //
        // Once a valid frame is received:
        //   map_ir_cmd(cmd_byte, &queue);
        //
        // Repeat frames (9ms burst + 2.25ms space) can be ignored or used for hold-repeat.

        tt.sleep_ms(10).ok();

        let _ = (queue.clone(), map_ir_cmd, pins::IR_PORT, pins::IR_PIN, NEC_ADDR); // keep reachable until wired
        let _ = (NEC_LEADER_PULSE_US, NEC_LEADER_SPACE_US, NEC_BIT_PULSE_US,
                 NEC_BIT_0_SPACE_US, NEC_BIT_1_SPACE_US, NEC_REPEAT_SPACE_US,
                 NEC_TIMING_MARGIN);
    }
}

/// Translate a decoded NEC command byte into an InputEvent and push it to the queue.
fn map_ir_cmd(cmd: u8, queue: &EventQueue) {
    let event = match cmd {
        IR_CMD_BRIGHTNESS_UP   => Some(InputEvent::BrightnessUp),
        IR_CMD_BRIGHTNESS_DOWN => Some(InputEvent::BrightnessDown),
        IR_CMD_PATTERN_NEXT    => Some(InputEvent::PatternNext),
        IR_CMD_PATTERN_PREV    => Some(InputEvent::PatternPrev),
        IR_CMD_HOLD            => Some(InputEvent::ToggleHold),
        IR_CMD_GEAR            => Some(InputEvent::CycleSoundMode),
        // TV button spare - add mapping once use is decided
        _                      => None,
    };
    if let Some(ev) = event {
        if let Ok(mut q) = queue.lock() {
            q.push_back(ev);
        }
    }
}
