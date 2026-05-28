pub mod map;

use map::LED_COUNT;

use crate::pins;


/// Abstracts over WS2812 hardware output and USB-serial previewer output.
/// Compile with `--features previewer` to target the previewer bridge instead of real LEDs.
pub struct LedOutput {
    inner: Inner,
}

#[cfg(not(feature = "previewer"))]
struct Inner {
    ws2812: bio_lib::ws2812::Ws2812,
}

#[cfg(feature = "previewer")]
struct Inner {
    usb: usb_bao1x::UsbHid,
    tt:  ticktimer::Ticktimer,
}

impl LedOutput {
    #[cfg(not(feature = "previewer"))]
    pub fn new() -> Self {
        let pin = arbitrary_int::u5::new(pins::LED_BIO_PIN);
        let ws2812 = bio_lib::ws2812::Ws2812::new(
            bio_lib::ws2812::LedVariant::B,
            pin,
            None,
        )
        .expect("failed to init WS2812 BIO driver");
        LedOutput { inner: Inner { ws2812 } }
    }

    #[cfg(feature = "previewer")]
    pub fn new() -> Self {
        let usb = usb_bao1x::UsbHid::new();
        let tt  = ticktimer::Ticktimer::new().unwrap();
        LedOutput { inner: Inner { usb, tt } }
    }

    /// Send one frame. `frame[i]` is `[r, g, b]` for the LED described by `LED_MAP[i]`.
    /// LED_MAP is sorted by boardId/localIdx, not by chainIdx, so we reorder before
    /// sending - the hardware and previewer bridge both expect bytes in chainIdx order.
    pub fn send_frame(&mut self, frame: &[[u8; 3]; LED_COUNT]) {
        // Reorder: chain_ordered[chainIdx] = colour for that physical chain position.
        let mut chain_ordered = [[0u8; 3]; LED_COUNT];
        for (i, rgb) in frame.iter().enumerate() {
            chain_ordered[map::LED_MAP[i].chain_idx as usize] = *rgb;
        }

        #[cfg(not(feature = "previewer"))]
        {
            let mut packed = [0u32; LED_COUNT];
            for (i, rgb) in chain_ordered.iter().enumerate() {
                packed[i] = bio_lib::ws2812::rgb_to_u32(rgb[0], rgb[1], rgb[2]);
            }
            self.inner.ws2812.send(&packed);
        }

        #[cfg(feature = "previewer")]
        {
            // 4-byte magic + 1800 RGB bytes in chain order.
            // Magic lets bridge.js sync to frame boundaries even if it connects mid-stream.
            // Must match FRAME_MAGIC in triangel previewer/bridge.js.
            // LED channels are clamped to 0-254 so 0xFF never appears in payload,
            // making the all-0xFF magic unambiguous.
            const MAGIC: [u8; 4] = [0xFF, 0xFF, 0xFF, 0xFF];
            let mut buf = [0u8; 4 + LED_COUNT * 3];
            buf[..4].copy_from_slice(&MAGIC);
            for (i, rgb) in chain_ordered.iter().enumerate() {
                buf[4 + i * 3]     = rgb[0].min(254);
                buf[4 + i * 3 + 1] = rgb[1].min(254);
                buf[4 + i * 3 + 2] = rgb[2].min(254);
            }
            // The USB CDC TX ring buffer is 1024 bytes; our frame is 1804. Sending in
            // 512-byte chunks with a 1ms yield between each lets the Xous USB server's
            // interrupt handler drain the ring buffer before the next chunk arrives.
            for chunk in buf.chunks(512) {
                self.inner.usb.serial_send(chunk).ok();
                self.inner.tt.sleep_ms(1).ok();
            }
        }
    }
}
