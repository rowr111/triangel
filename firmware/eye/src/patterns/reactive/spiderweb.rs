use core::f32::consts::PI;

use crate::audio::Audio;
use crate::led::map::{Led, LED_COUNT};
use crate::patterns::{Frame, ReactivePattern, hsv};

/// The tile outlines line up into straight rows running edge to edge across the fixture,
/// nine in each of three directions: horizontal, and the two slants at 60 degrees. Where
/// two tiles meet their outlines run side by side, so those count as two separate lines.
const LINES_PER_FAMILY: usize = 9;
const MAX_LINES: usize = LINES_PER_FAMILY * 3;
/// An LED's direction, from the two LEDs either side of it on its tile, has to be this
/// close to one of the three to count. Corners turn a sharp angle and fail it.
const DIR_TOLERANCE: f32 = 12.0 * PI / 180.0;
/// LEDs in one row share an offset to within this; neighboring rows are 12 mm apart.
const ROW_MERGE_MM: f32 = 1.0;

/// The whole web glows at this level between beats.
const BASE_LEVEL: f32 = 0.08;
/// How far a beat lifts that glow, and how long it takes to settle back. A beat takes the
/// background to BASE_LEVEL * (1 + PULSE_DEPTH), so the web breathes with the music.
const PULSE_DEPTH: f32 = 1.5;
const PULSE_MS: f32 = 300.0;

/// How long a lit line burns white, and how much brighter it is over that moment.
const HEAD_MS: f32 = 90.0;
const HEAD_BOOST: f32 = 1.5;
/// How long a lit line takes to fade back down to the background.
const FADE_MS: f32 = 2800.0;
/// Lines a beat lights beyond the first: none on a light beat, up to this on a full one.
const EXTRA_LINES: f32 = 2.0;

/// A drop lights every line at once, holds, then fades slowly.
const DROP_HOLD_MS: f32 = 700.0;
const DROP_FADE_MS: f32 = 2500.0;

/// Silk colors: mostly white, with pale blue, lavender and violet. Three of the six are
/// white, so half of all lit lines are.
const PALETTE: [(f32, f32); 6] =
    [(0.0, 0.0), (0.0, 0.0), (0.0, 0.0), (205.0, 0.30), (260.0, 0.32), (278.0, 0.50)];

#[derive(Clone, Copy)]
struct Shot {
    start_ms: u32,
    strength: f32,
    hue:      f32,
    sat:      f32,
    hold_ms:  f32,
    fade_ms:  f32,
}

const NO_SHOT: Shot =
    Shot { start_ms: 0, strength: 0.0, hue: 0.0, sat: 0.0, hold_ms: 0.0, fade_ms: 0.0 };

struct Line {
    family: usize,
    offset: f32,
    /// The current shot and the one before, both drawn, so a line lit again while still
    /// glowing keeps whichever is brighter rather than dropping back first.
    shots:  [Shot; 2],
}

/// A spider web. It rests at a faint glow that pulses up on each beat; on the beat a whole
/// line lights at once, burns white, then settles into its color and fades back down to
/// the glow. A drop lights every line at once.
pub struct Spiderweb {
    rng:      u32,
    lines:    Vec<Line>,
    /// Which line each LED sits on.
    led_line: [u8; LED_COUNT],
    /// When the last beat landed and how hard, for the background pulse.
    pulse_ms: u32,
    pulse:    f32,
}

impl Spiderweb {
    pub fn new(leds: &[Led]) -> Self {
        let normal: [(f32, f32); 3] = core::array::from_fn(|f| {
            let a = f as f32 * PI / 3.0;
            (-a.sin(), a.cos())
        });
        let across = |f: usize, led: &Led| normal[f].0 * led.wx + normal[f].1 * led.wy;

        // Which LED sits at each position on each tile, for finding an LED's neighbors.
        let mut at = [[u16::MAX; 26]; 26];
        for (i, led) in leds.iter().enumerate() {
            at[led.board_id as usize][led.local_idx as usize] = i as u16;
        }

        // Each LED's direction from the LEDs either side of it on its tile.
        let mut family = [u8::MAX; LED_COUNT];
        for (i, led) in leds.iter().enumerate() {
            let (b, l) = (led.board_id as usize, led.local_idx as usize);
            if l == 0 || l + 1 >= 26 {
                continue;
            }
            let (p, n) = (at[b][l - 1], at[b][l + 1]);
            if p == u16::MAX || n == u16::MAX {
                continue;
            }
            let (p, n) = (&leds[p as usize], &leds[n as usize]);
            let mut a = (n.wy - p.wy).atan2(n.wx - p.wx);
            if a < 0.0 {
                a += PI;
            }
            for f in 0..3 {
                let d = (a - f as f32 * PI / 3.0).abs();
                if d.min(PI - d) < DIR_TOLERANCE {
                    family[i] = f as u8;
                }
            }
        }

        // Rows are the LEDs of one direction sharing an offset. The largest nine in each
        // direction are the lines; anything smaller is a corner that slipped through.
        let mut lines = Vec::new();
        for f in 0..3 {
            let mut offs: Vec<f32> = leds
                .iter()
                .zip(family.iter())
                .filter(|&(_, &fam)| fam as usize == f)
                .map(|(led, _)| across(f, led))
                .collect();
            offs.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(core::cmp::Ordering::Equal));
            let mut rows: Vec<(usize, f32)> = Vec::new();
            let mut run = (0usize, 0.0f32, f32::MIN);
            for o in offs {
                if o - run.2 > ROW_MERGE_MM && run.0 > 0 {
                    rows.push((run.0, run.1 / run.0 as f32));
                    run = (0, 0.0, o);
                }
                run = (run.0 + 1, run.1 + o, o);
            }
            if run.0 > 0 {
                rows.push((run.0, run.1 / run.0 as f32));
            }
            rows.sort_unstable_by(|a, b| b.0.cmp(&a.0));
            for &(_, offset) in rows.iter().take(LINES_PER_FAMILY) {
                lines.push(Line { family: f, offset, shots: [NO_SHOT; 2] });
            }
        }

        // Every LED joins whichever line runs closest to it. That takes in the corners,
        // which sit where lines cross.
        let mut led_line = [0u8; LED_COUNT];
        for (i, led) in leds.iter().enumerate() {
            let mut best = (0usize, f32::MAX);
            for (li, line) in lines.iter().enumerate() {
                let d = (across(line.family, led) - line.offset).abs();
                if d < best.1 {
                    best = (li, d);
                }
            }
            led_line[i] = best.0 as u8;
        }

        Spiderweb { rng: 0x5157_3EB0, lines, led_line, pulse_ms: 0, pulse: 0.0 }
    }

    fn next_rng(&mut self) -> u32 {
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    fn randf(&mut self) -> f32 {
        (self.next_rng() >> 8) as f32 / 16_777_216.0
    }

    /// Light line `li`, replacing whichever of its two shots will finish sooner, so a long
    /// drop survives the beats that follow it.
    fn fire(&mut self, li: usize, t_ms: u32, strength: f32, hold: f32, fade: f32) {
        let (hue, sat) = PALETTE[(self.randf() * PALETTE.len() as f32) as usize % PALETTE.len()];
        let line = &mut self.lines[li];
        let remaining = |s: &Shot| {
            if s.strength <= 0.0 {
                f32::MIN
            } else {
                s.hold_ms + s.fade_ms - t_ms.wrapping_sub(s.start_ms) as i32 as f32
            }
        };
        let slot = if remaining(&line.shots[0]) <= remaining(&line.shots[1]) { 0 } else { 1 };
        line.shots[slot] = Shot { start_ms: t_ms, strength, hue, sat, hold_ms: hold, fade_ms: fade };
    }
}

impl ReactivePattern for Spiderweb {
    fn render(&mut self, _leds: &[Led], t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            for li in 0..self.lines.len() {
                self.fire(li, t_ms, 1.0, DROP_HOLD_MS, DROP_FADE_MS);
            }
        } else if audio.beat {
            let count = 1 + (audio.beat_strength * EXTRA_LINES) as usize;
            for _ in 0..count {
                let li = (self.randf() * self.lines.len() as f32) as usize % self.lines.len();
                self.fire(li, t_ms, 0.5 + 0.5 * audio.beat_strength, 0.0, FADE_MS);
            }
        }
        if audio.beat || audio.drop {
            self.pulse_ms = t_ms;
            self.pulse = if audio.drop { 1.0 } else { 0.5 + 0.5 * audio.beat_strength };
        }

        // The background swells on the beat and settles back.
        let since = t_ms.wrapping_sub(self.pulse_ms) as f32;
        let swell = self.pulse * (1.0 - since / PULSE_MS).max(0.0).powi(2);
        let base = BASE_LEVEL * (1.0 + PULSE_DEPTH * swell);

        // Every LED on a line shares its brightness, so each line's color is worked out
        // once and copied to its LEDs.
        let mut color = [[0u8; 3]; MAX_LINES];
        for (c, line) in color.iter_mut().zip(self.lines.iter()) {
            let mut best = (0.0f32, 0.0f32, 0.0f32); // brightness, hue, saturation
            for shot in &line.shots {
                if shot.strength <= 0.0 {
                    continue;
                }
                let age = t_ms.wrapping_sub(shot.start_ms) as i32 as f32;
                if age < 0.0 || age >= shot.hold_ms + shot.fade_ms {
                    continue;
                }
                let fade = if age < shot.hold_ms {
                    1.0
                } else {
                    1.0 - (age - shot.hold_ms) / shot.fade_ms
                };
                let head = (1.0 - age / HEAD_MS).clamp(0.0, 1.0);
                let v = (shot.strength * fade * fade * (1.0 + HEAD_BOOST * head)).min(1.0);
                if v > best.0 {
                    best = (v, shot.hue, shot.sat * (1.0 - head));
                }
            }
            // A lit line rises out of the background and settles back into it, taking on
            // its color only as far as it is lit, so the glow underneath stays white.
            let (lift, hue, sat) = best;
            *c = hsv(hue, sat * lift, (base + (1.0 - base) * lift).min(1.0));
        }

        for (o, &li) in out.iter_mut().zip(self.led_line.iter()) {
            *o = color[li as usize];
        }
    }
}
