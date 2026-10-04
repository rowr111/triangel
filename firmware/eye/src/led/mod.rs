pub mod geom;
pub mod grid;
pub mod map;
pub mod world;
#[cfg(not(feature = "previewer"))]
mod ws2812_pair;

use map::LED_COUNT;
#[cfg(not(feature = "previewer"))]
use map::{CHAIN1_LED_COUNT, CHAIN2_LED_COUNT};

#[cfg(not(feature = "previewer"))]
use crate::pins;


/// Sends frames to the LED chains, or over USB serial with `--features previewer`.
pub struct LedOutput {
    inner: Inner,
}

#[cfg(not(feature = "previewer"))]
struct Inner {
    ws2812: ws2812_pair::Ws2812Pair,
}

#[cfg(feature = "previewer")]
struct Inner {
    usb: usb_bao1x::UsbHid,
    tt:  ticktimer::Ticktimer,
}

impl LedOutput {
    #[cfg(not(feature = "previewer"))]
    pub fn new() -> Self {
        let pin1 = arbitrary_int::u5::new(pins::LED_BIO_PIN);
        let pin2 = arbitrary_int::u5::new(pins::LED_BIO_PIN_2);
        let ws2812 = ws2812_pair::Ws2812Pair::new(pin1, pin2)
            .expect("failed to init WS2812 BIO driver");
        LedOutput { inner: Inner { ws2812 } }
    }

    #[cfg(feature = "previewer")]
    pub fn new() -> Self {
        let usb = usb_bao1x::UsbHid::new();
        let tt  = ticktimer::Ticktimer::new().unwrap();
        LedOutput { inner: Inner { usb, tt } }
    }

    /// Sends one frame; `frame[i]` is the color of `LED_MAP[i]`. On hardware this takes
    /// about 9.4 ms.
    pub fn send_frame(&mut self, frame: &[[u8; 3]; LED_COUNT]) {
        // The chains and the previewer both want chain order, not LED_MAP order.
        let mut chain_ordered = [[0u8; 3]; LED_COUNT];
        for (i, rgb) in frame.iter().enumerate() {
            chain_ordered[map::LED_MAP[i].chain_idx as usize] = *rgb;
        }

        #[cfg(not(feature = "previewer"))]
        {
            // Chain 1 is chain_idx 0-287, chain 2 is 288-599.
            let mut packed1 = [0u32; CHAIN1_LED_COUNT];
            for (i, rgb) in chain_ordered[..CHAIN1_LED_COUNT].iter().enumerate() {
                packed1[i] = ws2812_pair::rgb_to_u32(rgb[0], rgb[1], rgb[2]);
            }
            let mut packed2 = [0u32; CHAIN2_LED_COUNT];
            for (i, rgb) in chain_ordered[CHAIN1_LED_COUNT..].iter().enumerate() {
                packed2[i] = ws2812_pair::rgb_to_u32(rgb[0], rgb[1], rgb[2]);
            }
            self.inner.ws2812.send_async(&packed1, &packed2);
            self.inner.ws2812.send_await();
        }

        #[cfg(feature = "previewer")]
        {
            // Frame start marker; must match FRAME_MAGIC in triangel previewer/bridge.js.
            // Colors are capped at 254 so the marker never appears in the data.
            const MAGIC: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
            let mut buf = [0u8; 4 + LED_COUNT * 3];
            buf[..4].copy_from_slice(&MAGIC);
            for (i, rgb) in chain_ordered.iter().enumerate() {
                buf[4 + i * 3]     = rgb[0].min(254);
                buf[4 + i * 3 + 1] = rgb[1].min(254);
                buf[4 + i * 3 + 2] = rgb[2].min(254);
            }
            // The USB send buffer is 1024 bytes and a frame is 1804, so send in chunks
            // with a pause for the buffer to drain.
            for chunk in buf.chunks(512) {
                self.inner.usb.serial_send(chunk).ok();
                self.inner.tt.sleep_ms(1).ok();
            }
        }
    }
}
