pub mod geom;
pub mod grid;
pub mod map;

use map::LED_COUNT;
#[cfg(not(feature = "previewer"))]
use map::{CHAIN1_LED_COUNT, CHAIN2_LED_COUNT};

#[cfg(not(feature = "previewer"))]
use crate::pins;


/// Abstracts over WS2812 hardware output and USB-serial previewer output.
/// Compile with `--features previewer` to target the previewer bridge instead of real LEDs.
pub struct LedOutput {
    inner: Inner,
}

#[cfg(not(feature = "previewer"))]
struct Inner {
    ws2812_1: bio_lib::ws2812::Ws2812,
    // None when init fails: bio-lib's Ws2812 claims Fifo1+Fifo2 in every
    // instance, so a second instance always gets ResourceInUse. Chain 2 stays
    // dark until the driver supports a second chain; boot must not die over it.
    ws2812_2: Option<bio_lib::ws2812::Ws2812>,
}

#[cfg(feature = "previewer")]
struct Inner {
    usb: usb_bao1x::UsbHid,
    tt:  ticktimer::Ticktimer,
}

impl Default for LedOutput {
    fn default() -> Self {
        Self::new()
    }
}

impl LedOutput {
    #[cfg(not(feature = "previewer"))]
    pub fn new() -> Self {
        let pin1 = arbitrary_int::u5::new(pins::LED_BIO_PIN);
        let pin2 = arbitrary_int::u5::new(pins::LED_BIO_PIN_2);
        let ws2812_1 = bio_lib::ws2812::Ws2812::new(
            bio_lib::ws2812::LedVariant::B,
            pin1,
            None,
        )
        .expect("failed to init WS2812 BIO driver (chain 1)");
        let ws2812_2 = match bio_lib::ws2812::Ws2812::new(
            bio_lib::ws2812::LedVariant::B,
            pin2,
            None,
        ) {
            Ok(ws) => Some(ws),
            Err(e) => {
                crate::diag::Diag::new()
                    .line(&format!("WS2812 chain 2 init failed ({:?}), running chain 1 only", e));
                log::warn!("WS2812 chain 2 init failed ({:?}), running chain 1 only", e);
                None
            }
        };
        LedOutput { inner: Inner { ws2812_1, ws2812_2 } }
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
    /// Hardware: two BIO cores stream both chains in parallel (~9ms vs 18ms single chain).
    pub fn send_frame(&mut self, frame: &[[u8; 3]; LED_COUNT]) {
        // Reorder: chain_ordered[chainIdx] = colour for that physical chain position.
        let mut chain_ordered = [[0u8; 3]; LED_COUNT];
        for (i, rgb) in frame.iter().enumerate() {
            chain_ordered[map::LED_MAP[i].chain_idx as usize] = *rgb;
        }

        #[cfg(not(feature = "previewer"))]
        {
            // Chain 1: chainIdx 0-287 (tiles 1-12)
            let mut packed1 = [0u32; CHAIN1_LED_COUNT];
            for (i, rgb) in chain_ordered[..CHAIN1_LED_COUNT].iter().enumerate() {
                packed1[i] = bio_lib::ws2812::rgb_to_u32(rgb[0], rgb[1], rgb[2]);
            }
            // Start both BIO cores simultaneously, then wait for both to finish.
            self.inner.ws2812_1.send_async(&packed1);
            if let Some(ws2812_2) = self.inner.ws2812_2.as_mut() {
                // Chain 2: chainIdx 288-599 (tiles 13-25), renumbered 0-311 for this chain
                let mut packed2 = [0u32; CHAIN2_LED_COUNT];
                for (i, rgb) in chain_ordered[CHAIN1_LED_COUNT..].iter().enumerate() {
                    packed2[i] = bio_lib::ws2812::rgb_to_u32(rgb[0], rgb[1], rgb[2]);
                }
                ws2812_2.send_async(&packed2);
            }
            self.inner.ws2812_1.send_await();
            if let Some(ws2812_2) = self.inner.ws2812_2.as_ref() {
                ws2812_2.send_await();
            }
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
