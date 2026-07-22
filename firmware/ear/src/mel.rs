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
//! This is doing exactly what a spectrum analyzer does: FFT the audio, group
//! the frequency bins into bands, display each band's energy level. Our 24 band
//! values are those energy levels; the LED patterns on the eye chip are the "display".
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
//! A normal FFT at 16 kHz with 512 samples gives 257 frequency bins each
//! 31 Hz wide. The problem is that human hearing doesn't care equally about
//! all 31 Hz-wide slices: we're very sensitive to differences at low frequencies
//! (100 Hz vs 200 Hz sounds huge) but barely notice differences at high
//! frequencies (7000 Hz vs 7100 Hz sounds the same). The mel scale compresses
//! high frequencies and expands low ones to match this perceptual reality.
//! Grouping FFT bins into mel-spaced triangular filters gives us 24 bands
//! that each carry roughly equal perceptual weight.
//!
//! # Processing pipeline
//!
//! ```text
//! raw i16 samples
//!   -> RMS for activity detection + overall level
//!   -> Hann window (reduces edge artifacts)
//!   -> 512-point FFT
//!   -> power spectrum (|X[k]|^2 for each bin)
//!   -> 24 triangular mel filters (weighted sum of power bins per band)
//!   -> log compression (matches perceived loudness)
//!   -> adaptive-gain normalize (fast-attack/slow-decay ceiling) so it stays
//!      interesting at any volume, preserving relative band loudness
//!   -> power-law shaping + per-band fast-rise/slow-fall smoothing -> u16
//!   -> MelFrame { bands: [u16; 24], level, activity }
//! ```

use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::sync::Arc;
use triangel_shared::mel::{MelFrame, MEL_BANDS};

use crate::audio::FFT_SIZE;

/// Audio sample rate in Hz. Must match what the microphone and ear_sim.py use.
const SAMPLE_RATE: f32 = 16_000.0;

/// Lowest frequency covered by the filterbank.
/// 31 Hz captures sub-bass and kick drum fundamentals (important for EDM).
/// Going lower than ~31 Hz isn't useful - that's our FFT bin size at 16 kHz/512
/// samples, so there's no frequency information below it.
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

/// Computes mel-frequency band energies and activity from raw audio samples.
///
/// Create once at startup with `MelProcessor::new()`, then call `process()`
/// on every incoming 512-sample frame.
pub struct MelProcessor {
    /// rustfft plan for a 512-point forward FFT.
    /// Plans are expensive to build (they pick the fastest algorithm for the
    /// size), so we build it once and reuse it every frame.
    fft: Arc<dyn Fft<f32>>,

    /// Precomputed Hann window coefficients, one per sample.
    ///
    /// A Hann window is a smooth bell curve (1 at the center, 0 at both ends).
    /// Multiplying samples by it before the FFT tapers the frame to zero at
    /// its edges, preventing "spectral leakage" - the artificial smearing of
    /// energy into neighboring frequency bins that happens when a signal
    /// isn't an exact multiple of the frame length.
    hann: [f32; FFT_SIZE],

    // Sparse triangular filter bank: each band is a list of (bin_index, weight).
    //
    // For each of the 24 mel bands we store only the FFT bins that overlap
    // with that band's triangular filter (most bins have zero weight and are
    // skipped). Each entry is (bin_index, weight) where weight is between
    // 0.0 and 1.0 from the triangle shape. Sparse storage avoids iterating
    // over all 257 bins for every band on every frame.
    filters: Vec<Vec<(usize, f32)>>,

    /// Scratch space that rustfft needs internally during the FFT.
    /// Pre-allocated once so the FFT itself makes no heap allocations per frame.
    scratch: Vec<Complex<f32>>,

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
    /// Build the FFT plan and precompute the Hann window and mel filter bank.
    /// Call once at startup - this allocates; `process()` does not.
    pub fn new() -> Self {
        // --- FFT plan ---
        // FftPlanner chooses the fastest algorithm for size 512 (Cooley-Tukey
        // radix-2, since 512 = 2^9).
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        // --- Hann window ---
        // w[n] = 0.5 * (1 - cos(2*pi*n / (N-1)))
        // Ranges from 0.0 at the edges to 1.0 at the center.
        let mut hann = [0f32; FFT_SIZE];
        for (i, w) in hann.iter_mut().enumerate() {
            *w = 0.5
                * (1.0
                    - (2.0 * std::f32::consts::PI * i as f32 / (FFT_SIZE - 1) as f32).cos());
        }

        // --- Mel filter bank construction ---
        //
        // Step 1: place MEL_BANDS + 2 = 26 equally-spaced points on the mel
        // axis between mel(MEL_LOW_HZ) and mel(MEL_HIGH_HZ). These become the left
        // edge, centre, and right edge of each triangular filter.
        //
        // We work in "FFT bin" units throughout (integer indices 0..256),
        // so we immediately convert each mel point to the nearest bin.
        let n_bins = FFT_SIZE / 2 + 1; // 257 unique bins from real FFT
        let mel_low = hz_to_mel(MEL_LOW_HZ);
        let mel_high = hz_to_mel(MEL_HIGH_HZ);
        let freq_per_bin = SAMPLE_RATE / FFT_SIZE as f32; // 31.25 Hz per bin at 16 kHz / 512

        // MEL_BANDS + 2 points linearly spaced in mel domain -> FFT bin indices
        let bin_points: Vec<usize> = (0..=MEL_BANDS + 1)
            .map(|i| {
                let mel =
                    mel_low + (mel_high - mel_low) * i as f32 / (MEL_BANDS + 1) as f32;
                (mel_to_hz(mel) / freq_per_bin).round() as usize
            })
            .collect();

        // Step 2: for each band m, build a triangular filter over bins
        // [bin_points[m] .. bin_points[m+2]] with peak at bin_points[m+1].
        //
        //  weight
        //    1 |        /\
        //      |       /  \
        //      |      /    \
        //    0 |-----/------\------> bin
        //      left  center  right
        //
        // We store only the (bin, weight) pairs where weight > 0 to keep the
        // per-frame dot product fast.

        // Build triangular filters as sparse (bin, weight) pairs
        let filters: Vec<Vec<(usize, f32)>> = (0..MEL_BANDS)
            .map(|m| {
                let left = bin_points[m];
                let center = bin_points[m + 1];
                let right = bin_points[m + 2];
                (left..=right.min(n_bins - 1))
                    .filter_map(|k| {
                        let w = if k <= center {
                            if center == left {
                                1.0
                            } else {
                                (k - left) as f32 / (center - left) as f32
                            }
                        } else if right == center {
                            0.0
                        } else {
                            (right - k) as f32 / (right - center) as f32
                        };
                        if w > 0.0 { Some((k, w)) } else { None }
                    })
                    .collect()
            })
            .collect();

        let scratch_len = fft.get_inplace_scratch_len();
        let scratch = vec![Complex::default(); scratch_len];

        Self {
            fft,
            hann,
            filters,
            scratch,
            smoothed_rms: 0.0,
            // Start low so the ceiling adapts upward over the first few frames.
            ceiling: -20.0,
            band_smooth: [0.0; MEL_BANDS],
        }
    }

    /// Process one 512-sample audio frame and return a `MelFrame`.
    ///
    /// Hot path (~30x/second). The FFT runs on the pre-allocated scratch, but the
    /// input/power buffers still allocate per call. Steps: broadband RMS for level
    /// and activity, Hann window, 512-point FFT, power spectrum, 24 triangular mel
    /// filters, log compression, a single adaptive-gain normalize (fast-attack/
    /// slow-decay ceiling), power-law shaping, and per-band fast-rise/slow-fall
    /// smoothing. The gain/shaping/smoothing constants are a first pass (see above).
    pub fn process(&mut self, samples: &[i16; FFT_SIZE]) -> MelFrame {
        // --- Activity + overall level (broadband RMS, before windowing) ---
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

        // --- Hann window + FFT ---
        // Multiply each sample by its Hann coefficient, normalize to float, pack as
        // a complex number (imag = 0). rustfft works in-place on complex buffers.
        let mut buf: Vec<Complex<f32>> = samples
            .iter()
            .zip(self.hann.iter())
            .map(|(&s, &w)| Complex { re: (s as f32 / 32768.0) * w, im: 0.0 })
            .collect();
        self.fft.process_with_scratch(&mut buf, &mut self.scratch);

        // Power spectrum |X[k]|^2, positive frequencies only (257 unique bins).
        let power: Vec<f32> = buf[..FFT_SIZE / 2 + 1].iter().map(|c| c.norm_sqr()).collect();

        // --- Mel filterbank -> log energy per band ---
        // Weighted sum of power bins per triangular filter, then natural log
        // (perceived loudness is ~logarithmic; 1e-10 guards log(0) on silence).
        let mut logmel = [0f32; MEL_BANDS];
        for (m, filter) in self.filters.iter().enumerate() {
            let energy: f32 = filter.iter().map(|&(k, w)| power[k] * w).sum();
            logmel[m] = (energy + 1e-10).ln();
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
