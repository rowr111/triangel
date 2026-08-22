//! Mel-frequency filterbank and activity detection.
//!
//! # What this does
//!
//! Every 32 ms the ear chip receives 512 audio samples from the microphone.
//! This module converts those raw samples into a compact 24-number summary
//! that describes how much energy is in each frequency region of the sound,
//! shaped to match how human hearing actually works.
//!
//! The result is a `MelFrame` containing 24 band values (u16 each), one overall
//! level, and an activity flag, which is then sent over UART to the eye chip.
//!
//! # Relation to a spectrum analyzer
//!
//! This is doing exactly what a hardware spectrum analyzer does: run the audio
//! through a bank of bandpass filters, measure how much energy comes out of each
//! one, display those levels. Our 24 band values are those energy levels; the LED
//! patterns on the eye chip are the "display".
//!
//! A filter bank is not the only way to get here - the textbook alternative is an
//! FFT followed by grouping the frequency bins into bands. The filter bank is used
//! instead because it needs no FFT library, no windowing, and no complex numbers:
//! each band is five multiplies per sample. It also runs continuously rather than
//! per-frame, so bands do not reset at frame boundaries. The tradeoff is softer
//! separation between neighboring bands, which does not matter for driving LEDs.
//!
//! The one difference from a simple spectrum analyzer is the mel scale. Most
//! cheap spectrum analyzers use linearly-spaced bands (equal Hz width each),
//! which means the first few bars cover all the bass while most of the display
//! is wasted on high frequencies that all sound similar. The mel scale spaces
//! the bands to match how hearing actually works - more bands in the low and
//! mid frequencies where music has most of its interesting structure, fewer in
//! the highs. The result is that all 24 bands carry roughly equal perceptual
//! weight, so the LED patterns react evenly across the full sonic range rather
//! than being dominated by whichever frequency happens to have the most energy.
//!
//! # Why the mel scale?
//!
//! The obvious way to place 24 bands between 31 Hz and 8 kHz is evenly in Hz,
//! about 330 Hz apart. The problem is that human hearing doesn't care equally
//! about all 330 Hz-wide slices: we're very sensitive to differences at low
//! frequencies (100 Hz vs 200 Hz sounds huge) but barely notice differences at
//! high frequencies (7000 Hz vs 7100 Hz sounds the same). Even spacing would put
//! all the bass in the first bar and waste most of the display on highs that all
//! sound alike. The mel scale compresses high frequencies and expands low ones to
//! match this perceptual reality, so all 24 bands carry roughly equal perceptual
//! weight and the LED patterns react evenly across the full sonic range.
//!
//! Spacing the filter centers on the mel scale makes the bands narrow at the
//! bottom and wide at the top: band 1 covers about 31-191 Hz while band 24 covers
//! about 6.4-8 kHz.
//!
//! # Processing pipeline
//!
//! ```text
//! raw i16 samples
//!   -> RMS for activity detection + overall level
//!   -> 24 mel-spaced bandpass filters, one second-order IIR section each
//!   -> square and average each filter's output over the frame (band energy)
//!   -> log compression (matches perceived loudness)
//!   -> adaptive-gain normalize (fast-attack/slow-decay ceiling) so it stays
//!      interesting at any volume, preserving relative band loudness
//!   -> power-law shaping + per-band fast-rise/slow-fall smoothing -> u16
//!   -> MelFrame { bands: [u16; 24], level, activity }
//! ```

use triangel_shared::mel::{MelFrame, MEL_BANDS};

use crate::audio::FFT_SIZE;

/// Audio sample rate in Hz. Must match what the microphone and ear_sim.py use.
const SAMPLE_RATE: f32 = 16_000.0;

/// Lowest frequency covered by the filterbank.
/// 31 Hz captures sub-bass and kick drum fundamentals (important for EDM).
/// Going lower than ~31 Hz isn't useful - one cycle at 31 Hz already fills the
/// whole 32 ms frame, so there's nothing below it a frame can resolve.
const MEL_LOW_HZ: f32 = 31.0;

/// Highest frequency covered. At 16 kHz the Nyquist limit is 8 kHz, so this
/// is the maximum we can represent.
const MEL_HIGH_HZ: f32 = 8_000.0;

// RMS threshold (0.0-1.0, normalized from i16) above which activity is flagged.
// 0.02 corresponds to roughly -34 dBFS - loud enough to be intentional music
// but quiet enough to catch soft passages.
const ACTIVITY_THRESHOLD: f32 = 0.02;

// Asymmetric envelope: fast attack, fast-ish decay.
// Attack 0.8: the smoothed RMS jumps to a loud transient within a frame or two.
// Decay 0.4: it falls back ~90% within ~5 frames (~160 ms), clearing between
// sounds without flickering inside a single beat.
const ACTIVITY_ATTACK: f32 = 0.8;
const ACTIVITY_DECAY: f32  = 0.4;

// --- Normalization / shaping constants ---
// These are a FIRST PASS, modeled on the blinky-badge (log domain, adaptive
// floor/ceiling, pow 1.4) and audio-reactive-led-strip (fast-attack/slow-decay
// gain follower). Expect to tune them once real mic audio is flowing.

/// Adaptive-gain ceiling follower: rises fast toward a louder spectrum peak,
/// drifts down slowly so quiet passages still fill the display.
const CEIL_ATTACK: f32 = 0.5;
const CEIL_DECAY: f32  = 0.01;
/// Log-energy span below the ceiling that maps to 0 (the visible dynamic range).
const DYNAMIC_RANGE: f32 = 8.0;
/// Power-law shaping (from the blinky-badge: expands the top, compresses the bottom).
const POWER_LAW: f32 = 1.4;
/// Per-band smoothing envelope: fast rise, slower fall.
const BAND_ATTACK: f32 = 0.6;
const BAND_DECAY: f32  = 0.25;
/// Maps the smoothed broadband RMS to the 0..1 overall level.
const LEVEL_GAIN: f32 = 10.0;

/// Convert a frequency in Hz to the mel scale.
///
/// The mel scale is a perceptual scale of pitches - equal distances on the
/// mel scale sound equally spaced to a human listener. This formula
/// (HTK definition) maps 0 Hz -> 0 mel, 1000 Hz -> ~1000 mel.
fn hz_to_mel(hz: f32) -> f32 {
    2595.0 * (1.0 + hz / 700.0).log10()
}

/// Inverse of `hz_to_mel` - convert a mel value back to Hz.
fn mel_to_hz(mel: f32) -> f32 {
    700.0 * (10f32.powf(mel / 2595.0) - 1.0)
}

/// One second-order IIR bandpass section: passes frequencies near its center and
/// attenuates everything else. Twenty-four of these side by side make the filter bank.
struct Biquad {
    b0: f32,
    a1: f32,
    a2: f32,
    /// Delay-line state, carried across samples and across frames.
    s1: f32,
    s2: f32,
}

impl Biquad {
    /// Constant-peak-gain bandpass (Audio EQ Cookbook) centered on `center_hz` with
    /// a -3 dB width of `bandwidth_hz`, normalized so the a0 coefficient is 1.
    fn bandpass(center_hz: f32, bandwidth_hz: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * center_hz / SAMPLE_RATE;
        // alpha = sin(w0) / 2Q, with Q = center / bandwidth.
        let alpha = w0.sin() * bandwidth_hz / (2.0 * center_hz);
        let a0 = 1.0 + alpha;
        Self { b0: alpha / a0, a1: -2.0 * w0.cos() / a0, a2: (1.0 - alpha) / a0, s1: 0.0, s2: 0.0 }
    }

    /// Feed one sample in, get that sample's filtered output back.
    #[inline]
    fn step(&mut self, x: f32) -> f32 {
        // Direct form II transposed. A bandpass has b1 = 0 and b2 = -b0, so the
        // single product b0*x serves both feed-forward taps: three multiplies total.
        let bx = self.b0 * x;
        let y = bx + self.s1;
        self.s1 = self.s2 - self.a1 * y;
        self.s2 = -bx - self.a2 * y;
        y
    }
}

/// The 24 mel-spaced bandpass filters plus one energy accumulator per band.
///
/// Samples go in one at a time and 24 band energies come out once per frame. That
/// narrow interface is deliberate: this loop is fixed-shape integer-friendly work
/// with no branching, so it is the piece that could move to a BIO co-processor core
/// later without disturbing anything downstream of it.
struct BandBank {
    filters: [Biquad; MEL_BANDS],
    /// Running sum of squared filter output per band, since the last `take_energies`.
    energy: [f32; MEL_BANDS],
    /// Samples accumulated into `energy`, so the sum can be turned into a mean.
    count: u32,
}

impl BandBank {
    /// Place MEL_BANDS + 2 points evenly on the mel axis between MEL_LOW_HZ and
    /// MEL_HIGH_HZ, then give band m a filter centered on point m+1 spanning points
    /// m..m+2. Q works out between roughly 0.7 at the bottom and 4.6 at the top.
    fn new() -> Self {
        let mel_low = hz_to_mel(MEL_LOW_HZ);
        let mel_high = hz_to_mel(MEL_HIGH_HZ);
        let edge_hz = |i: usize| {
            mel_to_hz(mel_low + (mel_high - mel_low) * i as f32 / (MEL_BANDS + 1) as f32)
        };
        let filters = std::array::from_fn(|m| {
            Biquad::bandpass(edge_hz(m + 1), edge_hz(m + 2) - edge_hz(m))
        });
        Self { filters, energy: [0.0; MEL_BANDS], count: 0 }
    }

    /// Run one sample through every band and accumulate its squared output.
    #[inline]
    fn push(&mut self, sample: f32) {
        for (filter, energy) in self.filters.iter_mut().zip(self.energy.iter_mut()) {
            let y = filter.step(sample);
            *energy += y * y;
        }
        self.count += 1;
    }

    /// Mean-square energy per band over the samples pushed since the last call,
    /// clearing the accumulators. Filter state is left alone, so the bands stay
    /// continuous across frame boundaries.
    fn take_energies(&mut self) -> [f32; MEL_BANDS] {
        let n = self.count.max(1) as f32;
        let out = std::array::from_fn(|m| self.energy[m] / n);
        self.energy = [0.0; MEL_BANDS];
        self.count = 0;
        out
    }
}

/// Computes mel-frequency band energies and activity from raw audio samples.
///
/// Create once at startup with `MelProcessor::new()`, then call `process()`
/// on every incoming 512-sample frame.
pub struct MelProcessor {
    /// The 24 mel-spaced bandpass filters and their per-frame energy accumulators.
    bank: BandBank,

    /// Exponentially-smoothed RMS level used for activity detection and the
    /// overall level. Updated every frame with asymmetric attack/decay.
    smoothed_rms: f32,

    /// Adaptive-gain ceiling: follows the spectrum peak (fast up, slow down).
    /// The normalization maps [ceiling - DYNAMIC_RANGE, ceiling] -> 0..1.
    ceiling: f32,

    /// Per-band smoothed output (0..1), updated with asymmetric attack/decay.
    band_smooth: [f32; MEL_BANDS],
}

impl MelProcessor {
    /// Build the mel bandpass filter bank. Call once at startup.
    pub fn new() -> Self {
        Self {
            bank: BandBank::new(),
            smoothed_rms: 0.0,
            // Start low so the ceiling adapts upward over the first few frames.
            ceiling: -20.0,
            band_smooth: [0.0; MEL_BANDS],
        }
    }

    /// Process one 512-sample audio frame and return a `MelFrame`.
    ///
    /// Hot path (~30x/second), and allocation-free. Steps: broadband RMS for level
    /// and activity, the 24 bandpass filters, log compression, a single adaptive-gain
    /// normalize (fast-attack/slow-decay ceiling), power-law shaping, and per-band
    /// fast-rise/slow-fall smoothing. The gain/shaping/smoothing constants are a
    /// first pass (see above).
    pub fn process(&mut self, samples: &[i16; FFT_SIZE]) -> MelFrame {
        // --- Activity + overall level (broadband RMS, straight off the raw samples) ---
        // RMS = sqrt(mean(sample^2)); i16 normalized to -1.0..1.0 by /32768.
        let rms = (samples
            .iter()
            .map(|&s| (s as f32 / 32768.0).powi(2))
            .sum::<f32>()
            / FFT_SIZE as f32)
            .sqrt();
        // Asymmetric smoothing: jump up fast on transients (attack), fall back
        // slowly (decay) so activity and level don't flicker between beats.
        if rms > self.smoothed_rms {
            self.smoothed_rms += ACTIVITY_ATTACK * (rms - self.smoothed_rms);
        } else {
            self.smoothed_rms += ACTIVITY_DECAY * (rms - self.smoothed_rms);
        }
        let activity = self.smoothed_rms > ACTIVITY_THRESHOLD;
        let level = ((self.smoothed_rms * LEVEL_GAIN).clamp(0.0, 1.0) * 65535.0) as u16;

        // --- Bandpass filter bank -> log energy per band ---
        // Every sample passes through all 24 filters; each band accumulates the square
        // of its own filter's output. No windowing is needed because the filters run
        // continuously rather than treating the frame as an isolated block.
        for &s in samples.iter() {
            self.bank.push(s as f32 / 32768.0);
        }
        // Natural log of each band's mean-square energy (perceived loudness is
        // ~logarithmic; 1e-10 guards log(0) on silence).
        let mut logmel = [0f32; MEL_BANDS];
        for (lm, e) in logmel.iter_mut().zip(self.bank.take_energies()) {
            *lm = (e + 1e-10).ln();
        }

        // --- Adaptive normalization ---
        // Track one ceiling across the whole spectrum (so relative band loudness is
        // preserved): it follows the peak quickly up and drifts down slowly. The
        // floor sits a fixed log span below it. This is what keeps the display
        // interesting at any volume instead of dark-when-quiet / full-when-loud.
        let peak = logmel.iter().copied().fold(f32::MIN, f32::max);
        if peak > self.ceiling {
            self.ceiling += CEIL_ATTACK * (peak - self.ceiling);
        } else {
            self.ceiling += CEIL_DECAY * (peak - self.ceiling);
        }
        let floor = self.ceiling - DYNAMIC_RANGE;
        let span = (self.ceiling - floor).max(1e-3);

        // --- Normalize -> power-law -> per-band smoothing -> u16 ---
        let mut bands = [0u16; MEL_BANDS];
        for (m, &lm) in logmel.iter().enumerate() {
            let norm = ((lm - floor) / span).clamp(0.0, 1.0);
            let shaped = norm.powf(POWER_LAW);
            let sm = &mut self.band_smooth[m];
            if shaped > *sm {
                *sm += BAND_ATTACK * (shaped - *sm);
            } else {
                *sm += BAND_DECAY * (shaped - *sm);
            }
            bands[m] = (*sm * 65535.0) as u16;
        }

        // FUTURE (2b): also compute the raw (non-normalized) bands and reductions
        // (bass/mid/treble sums, onset/beat) here and add them to the MelFrame.

        MelFrame { bands, level, activity }
    }
}
