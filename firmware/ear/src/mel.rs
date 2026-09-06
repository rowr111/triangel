//! Third-octave filterbank and activity detection.
//!
//! # What this does
//!
//! Every 32 ms the ear chip receives 512 audio samples from the microphone.
//! This module converts those raw samples into a compact 24-number summary
//! that describes how much energy is in each frequency region of the sound.
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
//! # Band spacing
//!
//! Neighboring bands sit a constant frequency ratio apart, so each covers the same
//! musical interval. 31 Hz to 8 kHz is almost exactly 8 octaves, so 24 bands makes
//! every band a third of an octave wide - the spacing music spectrum analyzers use,
//! because pitch is ratios rather than differences in Hz.
//!
//! It matters most at the bottom. A kick fundamental near 50 Hz and a speaking voice
//! near 120 Hz land four bands apart instead of sharing one.
//!
//! # Processing pipeline
//!
//! ```text
//! raw i16 samples
//!   -> RMS for activity detection + overall level
//!   -> 24 third-octave bandpass filters, one second-order IIR section each
//!   -> square and average each filter's output over the frame (band energy)
//!   -> convert to dB (matches perceived loudness)
//!   -> normalize against windowed references, gated by absolute level
//!   -> power-law shaping + per-band fast-rise/slow-fall smoothing -> u16
//!   -> MelFrame { bands: [u16; 24], level, activity }
//! ```

use triangel_shared::mel::{level_to_wire, norm_to_wire, MelFrame, LEVEL_DB_FLOOR, MEL_BANDS};

use crate::audio::{FFT_SIZE, SAMPLE_RATE_HZ};

/// Audio sample rate in Hz, as read_frame delivers it. Derived from the BIO clock
/// and the decimation, so the band centres cannot drift away from the real rate.
const SAMPLE_RATE: f32 = SAMPLE_RATE_HZ as f32;

/// Lowest frequency covered by the filterbank, and so what the innermost LEDs show.
/// 40 Hz puts a kick drum fundamental in the first bands rather than a ring outside
/// them. Below this there is little musical content a 32 ms frame can resolve.
const BAND_LOW_HZ: f32 = 40.0;

/// Highest frequency covered. At 16 kHz the Nyquist limit is 8 kHz, so this
/// is the maximum we can represent.
const BAND_HIGH_HZ: f32 = 8_000.0;

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

/// Samples averaged at each end of the window to form the low/high reference.
const EXTREME_COUNT: usize = 5;

/// Frames between reference recomputations (~256 ms).
const REFRESH_FRAMES: u32 = 8;

/// How fast the reference follows a recomputed target, up and down. Rising quickly
/// keeps it responsive to something louder; falling slowly hides the step as the
/// loudest samples age out of the window together.
const REF_RISE: f32 = 0.5;
const REF_FALL: f32 = 0.03;

/// History behind the band reference, ~3 s.
const BAND_WINDOW_FRAMES: usize = 94;

/// History behind the level reference, ~10 s. Longer than the band window so loud
/// and quiet passages still read differently rather than both being scaled to fill.
const LEVEL_WINDOW_FRAMES: usize = 312;

/// dB below the shared ceiling that maps to 0 - the visible depth of the spectrum.
const BAND_VISIBLE_RANGE_DB: f32 = 26.0;

/// How far each band is scaled to its own range instead of the shared one. At 0 the
/// bands keep exact relative loudness, so quiet ones sit permanently dark; at 1 every
/// band fills its own range and they all look equally busy.
const PER_BAND_MIX: f32 = 0.45;

/// History behind each band's own reference, ~1.5 s.
const BAND_OWN_WINDOW_FRAMES: usize = 48;

/// Floor on one band's measured span, so a steady band is not stretched to fill.
const BAND_MIN_SPAN_DB: f32 = 8.0;

/// Music loses roughly this much energy per octave above the bass. Without a matching
/// lift the treble bands sit 25-30 dB under the bass ones and normalize to black.
const TILT_DB_PER_OCTAVE: f32 = 4.5;

/// Band the tilt pivots around: below it bands are cut, above it lifted.
const TILT_PIVOT_BAND: f32 = 8.0;

/// Absolute level a band needs before any of the above applies, and the range it
/// fades in over. Without this a band holding nothing but noise has its few dB of
/// wobble stretched to a full swing, so it sits lit and mushy while a real hit in it
/// has no room left to show. The `n` console command reports the dB the bands
/// actually occupy, for setting these.
const GATE_FLOOR_DB: f32 = -78.0;
const GATE_KNEE_DB:  f32 = 12.0;

/// Floor on the level reference's span, so a silent room's noise floor is not
/// stretched to full scale.
const LEVEL_MIN_SPAN_DB: f32 = 6.0;

/// Power-law shaping: expands the top, compresses the bottom.
const POWER_LAW: f32 = 1.4;

/// Per-band smoothing envelope: fast rise, slower fall.
const BAND_ATTACK: f32 = 0.6;
const BAND_DECAY: f32  = 0.25;

/// Rolling low/high reference over the last `N` frames: the average of the
/// `EXTREME_COUNT` lowest and highest values in the window. A new extreme joins its
/// group immediately but only leaves when it ages out, so `N` sets how long a loud
/// moment keeps counting.
struct RangeTracker<const N: usize> {
    history: [f32; N],
    idx:     usize,
    /// Values pushed so far, capped at `N`; the rest of the array is still zeros.
    filled:  usize,
    /// Frames since the last recompute.
    age:     u32,
    /// What the window currently measures, and what `normalize` reads after gliding
    /// toward it.
    target_low:  f32,
    target_high: f32,
    low:         f32,
    high:        f32,
}

impl<const N: usize> RangeTracker<N> {
    fn new() -> Self {
        // age starts due, so the first push computes a reference immediately.
        Self {
            history: [0.0; N],
            idx: 0,
            filled: 0,
            age: REFRESH_FRAMES,
            target_low: 0.0,
            target_high: 0.0,
            low: 0.0,
            high: 0.0,
        }
    }

    fn push(&mut self, v: f32) {
        self.history[self.idx] = v;
        self.idx = (self.idx + 1) % N;
        if self.filled < N {
            self.filled += 1;
        }
        self.age += 1;
        if self.age >= REFRESH_FRAMES {
            self.age = 0;
            self.recompute();
        }
        if self.filled == 1 {
            // Start on the first real value. Gliding up from zero would leave the
            // reference far too loud for the first few seconds.
            self.low = self.target_low;
            self.high = self.target_high;
        } else {
            self.low = glide(self.low, self.target_low);
            self.high = glide(self.high, self.target_high);
        }
    }

    /// Each value is offered to a small sorted group and displaces the one it beats.
    fn recompute(&mut self) {
        if self.filled == 0 {
            return;
        }
        let count = EXTREME_COUNT.min(self.filled);
        let mut lowest  = [f32::MAX; EXTREME_COUNT];
        let mut highest = [f32::MIN; EXTREME_COUNT];
        for &sample in self.history[..self.filled].iter() {
            let mut v = sample;
            for slot in lowest[..count].iter_mut() {
                if v < *slot {
                    core::mem::swap(slot, &mut v);
                }
            }
            let mut v = sample;
            for slot in highest[..count].iter_mut() {
                if v > *slot {
                    core::mem::swap(slot, &mut v);
                }
            }
        }
        let inv = 1.0 / count as f32;
        self.target_low  = lowest[..count].iter().sum::<f32>()  * inv;
        self.target_high = highest[..count].iter().sum::<f32>() * inv;
    }

    /// Where `v` sits in the measured range, 0.0-1.0. A range narrower than
    /// `min_span` is widened to it.
    fn normalize(&self, v: f32, min_span: f32) -> f32 {
        let span = (self.high - self.low).max(min_span);
        ((v - self.low) / span).clamp(0.0, 1.0)
    }
}

/// Move a reference one step toward its target, quickly up and slowly down.
fn glide(current: f32, target: f32) -> f32 {
    let rate = if target > current { REF_RISE } else { REF_FALL };
    current + (target - current) * rate
}

/// Band edge `i` of the MEL_BANDS + 2 points between BAND_LOW_HZ and BAND_HIGH_HZ,
/// each a constant ratio above the last.
fn edge_hz(i: usize) -> f32 {
    BAND_LOW_HZ * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(i as f32 / (MEL_BANDS + 1) as f32)
}

/// Fraction bits for coefficients and for the signal path. The chip has no FPU, so
/// f32 here costs ~95 cycles an operation; these are plain integers instead.
/// Coefficients need Q30 because a1 reaches -1.937, and the signal needs the extra
/// range of Q28 because a full-scale tone on a band centre drives the state to 1.004.
const COEF_Q: u32 = 30;
const SIG_Q: u32 = 28;

/// One second-order IIR bandpass section: passes frequencies near its center and
/// attenuates everything else. Twenty-four of these side by side make the filter bank.
struct Biquad {
    b0: i32,
    a1: i32,
    a2: i32,
    /// Delay-line state, carried across samples and across frames.
    s1: i32,
    s2: i32,
}

impl Biquad {
    /// Constant-peak-gain bandpass (Audio EQ Cookbook) centered on `center_hz` with
    /// a -3 dB width of `bandwidth_hz`, normalized so the a0 coefficient is 1.
    /// Designed in f32 and quantized once - this runs 24 times at startup, not in
    /// the hot loop.
    fn bandpass(center_hz: f32, bandwidth_hz: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * center_hz / SAMPLE_RATE;
        // alpha = sin(w0) / 2Q, with Q = center / bandwidth.
        let alpha = w0.sin() * bandwidth_hz / (2.0 * center_hz);
        let a0 = 1.0 + alpha;
        let q = |v: f32| (v as f64 * (1i64 << COEF_Q) as f64).round() as i32;
        Self {
            b0: q(alpha / a0),
            a1: q(-2.0 * w0.cos() / a0),
            a2: q((1.0 - alpha) / a0),
            s1: 0,
            s2: 0,
        }
    }

    /// Q30 coefficient times Q28 signal, back to Q28.
    #[inline]
    fn mul(coef: i32, sig: i32) -> i32 { ((coef as i64 * sig as i64) >> COEF_Q) as i32 }

    /// Feed one sample in, get that sample's filtered output back.
    #[inline]
    fn step(&mut self, x: i32) -> i32 {
        // Direct form II transposed. A bandpass has b1 = 0 and b2 = -b0, so the
        // single product b0*x serves both feed-forward taps: three multiplies total.
        let bx = Self::mul(self.b0, x);
        let y = bx + self.s1;
        self.s1 = self.s2 - Self::mul(self.a1, y);
        self.s2 = -bx - Self::mul(self.a2, y);
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
    energy: [i64; MEL_BANDS],
    /// Samples accumulated into `energy`, so the sum can be turned into a mean.
    count: u32,
}

impl BandBank {
    /// Give band m a filter centered on edge m+1 spanning edges m..m+2. Constant
    /// ratio spacing means every band has the same Q, about 2.2, and they overlap
    /// enough to leave no gaps.
    fn new() -> Self {
        let filters = std::array::from_fn(|m| {
            Biquad::bandpass(edge_hz(m + 1), edge_hz(m + 2) - edge_hz(m))
        });
        Self { filters, energy: [0; MEL_BANDS], count: 0 }
    }

    /// Run one sample through every band and accumulate its squared output.
    #[inline]
    fn push(&mut self, sample: i32) {
        for (filter, energy) in self.filters.iter_mut().zip(self.energy.iter_mut()) {
            let y = filter.step(sample);
            // y*y is Q56; shift back to Q28 so a frame of sums cannot overflow i64.
            *energy += (y as i64 * y as i64) >> SIG_Q;
        }
        self.count += 1;
    }

    /// Mean-square energy per band over the samples pushed since the last call,
    /// clearing the accumulators. Filter state is left alone, so the bands stay
    /// continuous across frame boundaries.
    fn take_energies(&mut self) -> [f32; MEL_BANDS] {
        let scale = self.count.max(1) as f32 * (1i64 << SIG_Q) as f32;
        let out = std::array::from_fn(|m| self.energy[m] as f32 / scale);
        self.energy = [0; MEL_BANDS];
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

    /// Shared reference, fed the loudest band's dB each frame. Holds the bands in
    /// their true loudness order.
    band_ref: RangeTracker<BAND_WINDOW_FRAMES>,

    /// Each band's own reference, so a quiet band still moves rather than sitting flat.
    band_own: [RangeTracker<BAND_OWN_WINDOW_FRAMES>; MEL_BANDS],

    /// Per-band dB lift that cancels music's natural rolloff with frequency.
    tilt: [f32; MEL_BANDS],

    /// Quietest and loudest band of the last frame, for the console readout.
    band_lo: f32,
    band_hi: f32,

    /// Reference for `level_norm`, fed the broadband dBFS each frame.
    level_ref: RangeTracker<LEVEL_WINDOW_FRAMES>,

    /// Per-band smoothed output (0..1), updated with asymmetric attack/decay.
    band_smooth: [f32; MEL_BANDS],
}

impl MelProcessor {
    /// Build the mel bandpass filter bank. Call once at startup.
    pub fn new() -> Self {
        Self {
            bank: BandBank::new(),
            smoothed_rms: 0.0,
            band_ref: RangeTracker::new(),
            band_own: core::array::from_fn(|_| RangeTracker::new()),
            tilt: {
                let per_band =
                    (BAND_HIGH_HZ / BAND_LOW_HZ).log2() / (MEL_BANDS + 1) as f32;
                core::array::from_fn(|m| {
                    TILT_DB_PER_OCTAVE * (m as f32 - TILT_PIVOT_BAND) * per_band
                })
            },
            band_lo: 0.0,
            band_hi: 0.0,
            level_ref: RangeTracker::new(),
            band_smooth: [0.0; MEL_BANDS],
        }
    }

    /// Quietest and loudest band of the last frame, in dB, for the console readout.
    pub fn band_reference(&self) -> (f32, f32) { (self.band_lo, self.band_hi) }

    /// Live level reference (low, high) in dBFS, for the console readout.
    pub fn level_reference(&self) -> (f32, f32) { (self.level_ref.low, self.level_ref.high) }

    /// Process one 512-sample audio frame and return a `MelFrame`.
    ///
    /// Hot path (~30x/second), and allocation-free. Steps: broadband RMS for level
    /// and activity, the 24 bandpass filters, conversion to dB, a gated normalization
    /// against windowed references, power-law shaping, and per-band fast-rise/slow-fall
    /// smoothing.
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
        // RMS is already normalized to full scale, so dBFS needs no calibration.
        let dbfs = if self.smoothed_rms > 0.0 {
            20.0 * self.smoothed_rms.log10()
        } else {
            LEVEL_DB_FLOOR
        };
        let level = level_to_wire(dbfs);
        self.level_ref.push(dbfs);
        let level_norm = norm_to_wire(self.level_ref.normalize(dbfs, LEVEL_MIN_SPAN_DB));

        // --- Bandpass filter bank -> log energy per band ---
        // Every sample passes through all 24 filters; each band accumulates the square
        // of its own filter's output. No windowing is needed because the filters run
        // continuously rather than treating the frame as an isolated block.
        for &s in samples.iter() {
            // i16 -> Q28: s/32768 scaled by 2^28 is exactly s << 13.
            self.bank.push((s as i32) << (SIG_Q - 15));
        }
        // Each band's mean-square energy in dB (perceived loudness is ~logarithmic;
        // 1e-10 guards log(0) on silence, and floors the result at -100 dB).
        let mut band_db = [0f32; MEL_BANDS];
        for (b, e) in band_db.iter_mut().zip(self.bank.take_energies()) {
            *b = 10.0 * (e + 1e-10).log10();
        }

        // --- Adaptive normalization, under an absolute gate ---
        // Each band is scaled between the shared reference, which preserves relative
        // loudness, and its own, which keeps it moving. PER_BAND_MIX sets the balance.
        // The references see tilted levels; the gate sees true ones, since it is
        // asking whether the band holds anything above the microphone's noise.
        self.band_lo = band_db.iter().copied().fold(f32::MAX, f32::min);
        self.band_hi = band_db.iter().copied().fold(f32::MIN, f32::max);
        let tilted_peak = band_db
            .iter()
            .zip(self.tilt.iter())
            .fold(f32::MIN, |m, (&db, &t)| m.max(db + t));
        self.band_ref.push(tilted_peak);
        let shared_high = self.band_ref.high;
        let shared_low = shared_high - BAND_VISIBLE_RANGE_DB;

        // --- Normalize -> power-law -> per-band smoothing -> u16 ---
        let mut bands = [0u16; MEL_BANDS];
        for (m, &raw_db) in band_db.iter().enumerate() {
            let db = raw_db + self.tilt[m];
            let own = &mut self.band_own[m];
            own.push(db);
            let high = shared_high + (own.high - shared_high) * PER_BAND_MIX;
            let low = shared_low + (own.low - shared_low) * PER_BAND_MIX;
            let gate = ((raw_db - GATE_FLOOR_DB) / GATE_KNEE_DB).clamp(0.0, 1.0);
            let norm = ((db - low) / (high - low).max(BAND_MIN_SPAN_DB)).clamp(0.0, 1.0) * gate;
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

        MelFrame { bands, level, level_norm, activity }
    }
}
