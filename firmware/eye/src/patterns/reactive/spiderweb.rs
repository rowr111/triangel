use core::cmp::Reverse;
use core::f32::consts::PI;

use crate::audio::Audio;
use crate::led::geom::{BOARD_COUNT, LEDS_PER_BOARD};
use crate::led::map::{Led, LED_COUNT, LED_MAP};
use crate::patterns::{hsv, Frame, Pattern, Rng, Shot, ShotPair};
use triangel_shared::tuning::spiderweb::*;

/// The tile outlines form straight lines across the fixture, nine in each of three
/// directions: horizontal and the two 60-degree slants.
const LINES_PER_FAMILY: usize = 9;
const MAX_LINES: usize = LINES_PER_FAMILY * 3;
/// How close an LED's direction must be to one of the three.
const DIR_TOLERANCE: f32 = 12.0 * PI / 180.0;
/// LEDs in one row share an offset to within this; neighboring rows are 12 mm apart.
const ROW_MERGE_MM: f32 = 1.0;
/// Most lines one beat lights.
const MAX_BEAT_LINES: usize = 1 + EXTRA_LINES as usize;

/// (hue, sat): half white, plus pale blue, lavender and violet.
const PALETTE: [(f32, f32); 6] = [
    (0.0, 0.0),
    (0.0, 0.0),
    (0.0, 0.0),
    (205.0, 0.30),
    (260.0, 0.32),
    (278.0, 0.50),
];

struct Line {
    /// The line is where `across(normal, led)` equals `offset`.
    normal: (f32, f32),
    offset: f32,
    /// The latest two shots; the brighter one shows.
    shots: ShotPair,
}

/// A faint web that pulses on each beat. A beat lights whole lines, white at first, then
/// fading through their color; a drop lights every line.
pub struct Spiderweb {
    rng: Rng,
    lines: Vec<Line>,
    /// Which line each LED is on.
    led_line: [u8; LED_COUNT],
    /// When the last beat came and how hard.
    pulse_ms: u32,
    pulse: f32,
}

impl Spiderweb {
    pub fn new() -> Self {
        let families = led_families();
        let lines: Vec<Line> = (0..3)
            .flat_map(|family| find_lines(family, &families))
            .collect();
        let led_line = core::array::from_fn(|i| nearest_line(&lines, &LED_MAP[i]));
        Spiderweb {
            rng: Rng::new(0x5157_3EB0),
            lines,
            led_line,
            pulse_ms: 0,
            pulse: 0.0,
        }
    }
}

impl Pattern for Spiderweb {
    fn on_enter(&mut self, _t_ms: u32) {
        for line in &mut self.lines {
            line.shots = ShotPair::NONE;
        }
    }

    fn render(&mut self, t_ms: u32, audio: &Audio, out: &mut Frame) {
        if audio.drop {
            self.on_drop(t_ms);
        } else if audio.beat {
            self.on_beat(t_ms, audio.beat_strength);
        }

        // Background glow, brighter just after a beat.
        let since = t_ms.wrapping_sub(self.pulse_ms) as f32;
        let swell = self.pulse * (1.0 - since / PULSE_MS).max(0.0).powi(2);
        let base = BASE_LEVEL * (1.0 + PULSE_DEPTH * swell);

        // One color per line, shared by all its LEDs.
        let mut color = [[0u8; 3]; MAX_LINES];
        for (c, line) in color.iter_mut().zip(self.lines.iter_mut()) {
            let shot = line.shots.brightest(t_ms).unwrap_or_default();
            // Saturation follows the lift, so the background glow stays white.
            let lift = shot.brightness;
            let brightness = (base + (1.0 - base) * lift).min(1.0);
            *c = hsv(shot.hue, shot.sat * lift, brightness);
        }

        for (o, &line) in out.iter_mut().zip(self.led_line.iter()) {
            *o = color[line as usize];
        }
    }
}

impl Spiderweb {
    /// Lights every line at full.
    fn on_drop(&mut self, t_ms: u32) {
        for line in 0..self.lines.len() {
            self.fire(line, t_ms, 1.0, DROP_HOLD_MS, DROP_FADE_MS);
        }
        self.pulse_ms = t_ms;
        self.pulse = 1.0;
    }

    /// Lights one line, and up to EXTRA_LINES more on a hard beat.
    fn on_beat(&mut self, t_ms: u32, beat_strength: f32) {
        let strength = 0.5 + 0.5 * beat_strength;
        let count = 1 + (beat_strength * EXTRA_LINES) as usize;
        let mut picked = [usize::MAX; MAX_BEAT_LINES];
        for i in 0..count {
            let mut line = self.rng.below(self.lines.len());
            // Pick again if this beat has already lit that line.
            while picked[..i].contains(&line) {
                line = self.rng.below(self.lines.len());
            }
            picked[i] = line;
            self.fire(line, t_ms, strength, 0.0, FADE_MS);
        }
        self.pulse_ms = t_ms;
        self.pulse = strength;
    }

    /// Lights `line`, replacing the shot that ends sooner, so a drop outlasts the beats after it.
    fn fire(&mut self, line: usize, t_ms: u32, strength: f32, hold_ms: f32, fade_ms: f32) {
        let (hue, sat) = self.rng.pick(&PALETTE);
        let shot = Shot {
            start_ms: t_ms,
            strength,
            hue,
            sat,
            hold_ms,
            fade_ms,
            white_ms: WHITE_MS,
            boost: HIT_BOOST,
        };
        self.lines[line].shots.add(shot, t_ms);
    }
}

/// The angle of the lines in direction `family`.
fn direction(family: u8) -> f32 {
    family as f32 * PI / 3.0
}

/// How far `led` is along `normal`.
fn across(normal: (f32, f32), led: &Led) -> f32 {
    normal.0 * led.wx + normal.1 * led.wy
}

/// Each LED's direction from the LEDs either side of it on its tile.
fn led_families() -> [Option<u8>; LED_COUNT] {
    // Which LED is at each position on each tile. Positions start at 1, and there is a
    // spare empty slot past the last.
    let mut led_at = [[u16::MAX; LEDS_PER_BOARD + 2]; BOARD_COUNT + 1];
    for (i, led) in LED_MAP.iter().enumerate() {
        led_at[led.board_id as usize][led.local_idx as usize] = i as u16;
    }

    core::array::from_fn(|i| {
        let led = &LED_MAP[i];
        let tile = &led_at[led.board_id as usize];
        let pos = led.local_idx as usize;
        if pos == 0 || pos + 1 >= tile.len() {
            return None;
        }
        let (prev, next) = (tile[pos - 1], tile[pos + 1]);
        if prev == u16::MAX || next == u16::MAX {
            return None;
        }
        let (prev, next) = (&LED_MAP[prev as usize], &LED_MAP[next as usize]);
        let mut angle = (next.wy - prev.wy).atan2(next.wx - prev.wx);
        if angle < 0.0 {
            angle += PI;
        }
        (0..3).find(|&family| {
            let off = (angle - direction(family)).abs();
            off.min(PI - off) < DIR_TOLERANCE
        })
    })
}

/// Groups the LEDs running in direction `family` into rows by offset. The nine largest
/// rows are the lines; smaller ones are stray corner LEDs.
fn find_lines(family: u8, families: &[Option<u8>; LED_COUNT]) -> Vec<Line> {
    let angle = direction(family);
    let normal = (-angle.sin(), angle.cos());
    let mut offsets: Vec<f32> = LED_MAP
        .iter()
        .zip(families)
        .filter(|&(_, &f)| f == Some(family))
        .map(|(led, _)| across(normal, led))
        .collect();
    offsets.sort_unstable_by(|a, b| a.total_cmp(b));
    let mut rows: Vec<&[f32]> = offsets.chunk_by(|a, b| b - a <= ROW_MERGE_MM).collect();
    rows.sort_unstable_by_key(|row| Reverse(row.len()));
    rows.iter()
        .take(LINES_PER_FAMILY)
        .map(|row| Line {
            normal,
            offset: row.iter().sum::<f32>() / row.len() as f32,
            shots: ShotPair::NONE,
        })
        .collect()
}

/// Index of the line nearest to `led`. Every LED, corners included, joins one.
fn nearest_line(lines: &[Line], led: &Led) -> u8 {
    let distance = |line: &Line| (across(line.normal, led) - line.offset).abs();
    (0..lines.len())
        .min_by(|&a, &b| distance(&lines[a]).total_cmp(&distance(&lines[b])))
        .unwrap_or(0) as u8
}
