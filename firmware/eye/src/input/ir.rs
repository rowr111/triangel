use super::{EventQueue, InputEvent};
use crate::pins;
use crate::setlist::SoundMode;

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

// NEC command byte -> function mapping.
// These are placeholder codes - capture the actual codes from the chosen remote
// with a logic analyser or by running the NEC decoder and logging received bytes.
const IR_CMD_BRIGHTNESS_UP:   u8 = 0x40;
const IR_CMD_BRIGHTNESS_DOWN: u8 = 0x41;
const IR_CMD_PATTERN_NEXT:    u8 = 0x42;
const IR_CMD_PATTERN_PREV:    u8 = 0x43;
const IR_CMD_HOLD:            u8 = 0x44;
const IR_CMD_SOUND_OFF:       u8 = 0x45;
const IR_CMD_SOUND_AUTO:      u8 = 0x46;
const IR_CMD_SOUND_ON:        u8 = 0x47;

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

        let _ = (queue.clone(), map_ir_cmd, pins::IR_PORT, pins::IR_PIN); // keep reachable until wired
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
        IR_CMD_SOUND_OFF       => Some(InputEvent::SetSoundMode(SoundMode::Off)),
        IR_CMD_SOUND_AUTO      => Some(InputEvent::SetSoundMode(SoundMode::Auto)),
        IR_CMD_SOUND_ON        => Some(InputEvent::SetSoundMode(SoundMode::On)),
        _                      => None,
    };
    if let Some(ev) = event {
        if let Ok(mut q) = queue.lock() {
            q.push_back(ev);
        }
    }
}
