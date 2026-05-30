use std::sync::atomic::Ordering;

use super::{Frame, Pattern};
use crate::audio::{
    STATUS_CSR_FAIL, STATUS_DMA_DONE, STATUS_IFRAM_FAIL, STATUS_INIT_OK, STATUS_RECEIVING,
    UART_FIRST_BYTE, UART_LAST_FRAME_MS, UART_STATUS,
};
use crate::led::map::{Led, WORLD_BOT, WORLD_H, WORLD_TOP};

/// Fills the triangle from the apex upward proportional to sound level, with
/// brightness also scaling with sound level. Loud = more LEDs lit AND brighter.
pub struct AudioFill;

impl Pattern for AudioFill {
    fn render(&mut self, leds: &[Led], t_ms: u32, sound_level: f32, out: &mut Frame) {
        let threshold_y = WORLD_BOT - sound_level * WORLD_H;
        let brightness  = sound_level;
        for (i, led) in leds.iter().enumerate() {
            if led.wy >= threshold_y {
                out[i] = [
                    (0.4 * brightness * 255.0) as u8,
                    (0.1 * brightness * 255.0) as u8,
                    (brightness * 255.0) as u8,
                ];
            } else {
                out[i] = [0, 0, 0];
            }
        }

        // debug: triangle 2 shows UART init status
        //   grey    = pending (init not yet attempted)
        //   magenta = CSR map failed (UART2 registers owned by another process)
        //   yellow  = IFRAM map failed
        //   blue    = init OK, waiting for first frame
        //   green   = receiving frames from ear chip
        let stale = UART_STATUS.load(Ordering::Relaxed) == STATUS_RECEIVING
            && t_ms.wrapping_sub(UART_LAST_FRAME_MS.load(Ordering::Relaxed)) > 500;
        let status_color: [u8; 3] = if stale { [0, 100, 200] } else { match UART_STATUS.load(Ordering::Relaxed) {
            STATUS_CSR_FAIL   => [200, 0,   200], // magenta  - UART2 owned by another process
            STATUS_IFRAM_FAIL => [200, 200, 0  ], // yellow   - IFRAM map failed
            STATUS_INIT_OK    => [0,   100, 200], // blue     - waiting for first byte
            STATUS_DMA_DONE   => match UART_FIRST_BYTE.load(Ordering::Relaxed) {
                0x00 => [150, 0,   150], // purple - buf[0]=0x00, UART idle or nothing sending
                0xAA => [200, 200, 0  ], // yellow - buf[0]=sync byte, checksum failing
                0xFF => [200, 200, 200], // white  - buf[0]=0xFF, line stuck high
                _    => [255, 80,  0  ], // orange - buf[0]=other, wrong data/baud
            },
            STATUS_RECEIVING  => [0,   200, 50 ], // green    - receiving good frames
            _                 => [60,  60,  60 ], // grey     - pending
        } };
        for (i, led) in leds.iter().enumerate() {
            if led.board_id == 2 { out[i] = status_color; }
        }

        // debug: top-left corner LED always red so we can confirm frames are live
        if let Some(i) = leds.iter().position(|l| l.wy <= WORLD_TOP + 1.0 && l.wx < 15.0) {
            out[i] = [255, 0, 0];
        }

        // debug: three adjacent LEDs chase 1-2-3 to confirm animation is running
        let step = (t_ms / 400) % 4; // 4 steps: 3 lit + 1 gap
        for (i, led) in leds.iter().enumerate() {
            if led.wy > WORLD_TOP + 1.0 || led.wx < 15.0 || led.wx >= 47.0 {
                continue;
            }
            let pos = if led.wx < 26.0 { 0 } else if led.wx < 36.0 { 1 } else { 2 };
            out[i] = if step == pos { [0, 180, 60] } else { [0, 0, 0] };
        }
    }
}
