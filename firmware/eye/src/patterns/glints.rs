use crate::led::map::LED_COUNT;
use crate::patterns::{lerp, time_step_ms, Frame, Rng};

const MAX_GLINTS: usize = 24;

/// How a pattern's glints look.
#[derive(Clone, Copy)]
pub struct GlintStyle {
    /// Tries per second. A try on a brighter LED is likelier to succeed.
    pub tries_per_sec: f32,
    /// Fade time range.
    pub min_life_ms: u32,
    pub max_life_ms: u32,
    /// How far toward white a glint starts.
    pub white: f32,
}

#[derive(Clone, Copy)]
struct Glint {
    led: u16,
    start_ms: u32,
    life_ms: u32,
}

/// Brief near-white flashes on single LEDs, mostly the bright ones. The pattern draws its
/// frame first, then calls `update` on it.
pub struct Glints {
    style: GlintStyle,
    rng: Rng,
    glints: [Glint; MAX_GLINTS],
    tries: f32, // fractional tries carried to the next frame
    last_ms: u32,
}

impl Glints {
    pub fn new(seed: u32, style: GlintStyle) -> Self {
        Glints {
            style,
            rng: Rng::new(seed),
            glints: [Glint {
                led: 0,
                start_ms: 0,
                life_ms: 0,
            }; MAX_GLINTS],
            tries: 0.0,
            last_ms: 0,
        }
    }

    /// Starts new glints, then blends every live one toward white.
    pub fn update(&mut self, t_ms: u32, out: &mut Frame) {
        let style = self.style;
        // Capped so a long gap cannot start a burst of tries.
        let dt_ms = time_step_ms(&mut self.last_ms, t_ms, 100);
        self.tries += dt_ms as f32 * style.tries_per_sec / 1000.0;
        while self.tries >= 1.0 {
            self.tries -= 1.0;
            let led = self.rng.below(LED_COUNT);
            let [r, g, b] = out[led];
            let lit = r.max(g).max(b) as f32 / 255.0;
            if self.rng.f32() >= lit {
                continue;
            }
            let free = self
                .glints
                .iter()
                .position(|glint| t_ms.wrapping_sub(glint.start_ms) >= glint.life_ms);
            if let Some(slot) = free {
                let life_ms = self.rng.range_u32(style.min_life_ms, style.max_life_ms);
                self.glints[slot] = Glint {
                    led: led as u16,
                    start_ms: t_ms,
                    life_ms,
                };
            }
        }

        for glint in &self.glints {
            let age = t_ms.wrapping_sub(glint.start_ms);
            if age >= glint.life_ms {
                continue;
            }
            let fade = 1.0 - age as f32 / glint.life_ms as f32;
            let whiten = fade * fade * style.white;
            for channel in out[glint.led as usize].iter_mut() {
                *channel = lerp(*channel as f32, 255.0, whiten) as u8;
            }
        }
    }
}
