use crate::led::map::LED_COUNT;
use crate::patterns::{Frame, Rng};

const MAX_GLINTS: usize = 24;

/// How a pattern's glints look.
#[derive(Clone, Copy)]
pub struct GlintStyle {
    /// Tries per second. Each lands only as likely as its LED is bright.
    pub tries_per_sec: f32,
    /// Fade time range, picked per glint.
    pub min_ms: u32,
    pub max_ms: u32,
    /// How far toward white a glint starts.
    pub white: f32,
}

#[derive(Clone, Copy)]
struct Glint {
    led:      u16,
    start_ms: u32,
    life_ms:  u32,
}

/// Brief near-white flashes on single LEDs, landing mostly on the brightest ones, shared by
/// Shimmer and Rainbow. The pattern draws its frame first, then calls `update` over it.
pub struct Glints {
    style:  GlintStyle,
    rng:    Rng,
    glints: [Glint; MAX_GLINTS],
    tries:  f32, // fractional tries carried to the next frame
}

impl Glints {
    pub fn new(seed: u32, style: GlintStyle) -> Self {
        Glints {
            style,
            rng: Rng::new(seed),
            glints: [Glint { led: 0, start_ms: 0, life_ms: 0 }; MAX_GLINTS],
            tries: 0.0,
        }
    }

    /// Land new glints, most on the brightest LEDs, then lift every live one toward white.
    pub fn update(&mut self, t_ms: u32, dt_ms: u32, out: &mut Frame) {
        let s = self.style;
        self.tries += dt_ms as f32 * s.tries_per_sec / 1000.0;
        while self.tries >= 1.0 {
            self.tries -= 1.0;
            let led = (self.rng.f32() * LED_COUNT as f32) as usize % LED_COUNT;
            let [r, g, b] = out[led];
            let lit = r.max(g).max(b) as f32 / 255.0;
            if self.rng.f32() >= lit {
                continue;
            }
            let free = self.glints.iter().position(|gl| t_ms.wrapping_sub(gl.start_ms) >= gl.life_ms);
            if let Some(slot) = free {
                let life_ms = s.min_ms + (self.rng.f32() * (s.max_ms - s.min_ms) as f32) as u32;
                self.glints[slot] = Glint { led: led as u16, start_ms: t_ms, life_ms };
            }
        }

        for gl in &self.glints {
            let age = t_ms.wrapping_sub(gl.start_ms);
            if age >= gl.life_ms {
                continue;
            }
            let fade = 1.0 - age as f32 / gl.life_ms as f32;
            let k = fade * fade * s.white;
            for c in out[gl.led as usize].iter_mut() {
                *c = (*c as f32 + (255.0 - *c as f32) * k) as u8;
            }
        }
    }
}
