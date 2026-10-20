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

// send_frame splits the packed frame at the end of chain 1.
#[cfg(not(feature = "previewer"))]
const _: () = assert!(CHAIN1_LED_COUNT + CHAIN2_LED_COUNT == LED_COUNT);

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
    /// about 11 ms.
    pub fn send_frame(&mut self, frame: &[[u8; 3]; LED_COUNT]) {
        // LED_MAP is in chain order, so the frame goes out in the order it is.
        #[cfg(not(feature = "previewer"))]
        {
            // Chain 1 is LEDs 0-287, chain 2 is 288-599.
            let packed: [u32; LED_COUNT] = core::array::from_fn(|i| {
                let rgb = frame[i];
                ws2812_pair::rgb_to_u32(rgb[0], rgb[1], rgb[2])
            });
            let (chain1, chain2) = packed.split_at(CHAIN1_LED_COUNT);
            self.inner.ws2812.send_async(chain1, chain2);
            self.inner.ws2812.send_await();
        }

        #[cfg(feature = "previewer")]
        {
            // Frame start marker; must match FRAME_MAGIC in triangel-previewer/bridge.js.
            // Colors are capped at 254 so the marker never appears in the data.
            const MAGIC: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
            let mut buf = [0u8; 4 + LED_COUNT * 3];
            buf[..4].copy_from_slice(&MAGIC);
            for (px, rgb) in buf[4..].chunks_exact_mut(3).zip(frame) {
                px[0] = rgb[0].min(254);
                px[1] = rgb[1].min(254);
                px[2] = rgb[2].min(254);
            }
            // The USB send buffer is 1024 bytes and a frame is 1804, so send in chunks
            // with a pause for the buffer to drain.
            for chunk in buf.chunks(512) {
                // A short write: the PC is not reading. The bridge drops a cut frame.
                if !matches!(self.inner.usb.serial_send(chunk), Ok(n) if n == chunk.len()) {
                    break;
                }
                self.inner.tt.sleep_ms(1).ok();
            }
        }
    }
}
