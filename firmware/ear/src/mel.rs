//! Turns each frame of microphone samples into a `MelFrame`: the level in 24 frequency
//! bands from 40 Hz to 12 kHz, plus the overall level.
//!
//! Each band is a third of an octave wide: its edges are a fixed ratio apart, not a fixed
//! number of Hz. Equal-width bands, or the mel scale, would put a kick drum near 50 Hz
//! and a voice near 120 Hz in the same band. Here they land four bands apart.

use triangel_shared::mel::{level_to_wire, norm_to_wire, MelFrame, LEVEL_DB_FLOOR, MEL_BANDS};
use triangel_shared::follow;
use triangel_shared::tuning::ear::*;

use crate::audio::{FFT_SIZE, SAMPLE_RATE_HZ};

/// Sample rate of the frames read_frame delivers, in Hz.
const SAMPLE_RATE: f32 = SAMPLE_RATE_HZ as f32;

/// Lowest frequency covered. 40 Hz puts a kick drum's fundamental in the first bands.
const BAND_LOW_HZ: f32 = 40.0;

/// Highest frequency covered: half the 24 kHz sample rate.
const BAND_HIGH_HZ: f32 = 12_000.0;

// How fast the smoothed RMS behind the level rises and falls, per frame.
const LEVEL_ATTACK: f32 = 0.8;
const LEVEL_DECAY: f32  = 0.4;

// The values meant for tuning come from `triangel_shared::tuning::ear`.

/// Values kept at each end of the window. The reference is the last of them, so one
/// loud spike cannot move it.
const EXTREME_COUNT: usize = 5;

/// Frames between reference recomputations (~256 ms).
const REFRESH_FRAMES: u32 = 8;

/// Bands averaged for the raw bass level: 40-100 Hz, the kick and the bass line.
const BASS_BANDS: usize = 3;

/// Low and high reference over the last `N` frames: the `EXTREME_COUNT`th lowest and
/// highest values in the window.
struct RangeTracker<const N: usize> {
    history: [f32; N],
    idx:     usize,
    /// Values pushed so far, capped at `N`.
    filled:  usize,
    /// Frames since the last recompute.
    age:     u32,
    /// What the window measures now; `low` and `high` follow it.
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
            // Start on the first value. Rising from zero would leave the reference wrong
            // for the first few seconds.
            self.low = self.target_low;
            self.high = self.target_high;
        } else {
            self.low = follow(self.low, self.target_low, REF_RISE, REF_FALL);
            self.high = follow(self.high, self.target_high, REF_RISE, REF_FALL);
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
                    std::mem::swap(slot, &mut v);
                }
            }
            let mut v = sample;
            for slot in highest[..count].iter_mut() {
                if v > *slot {
                    std::mem::swap(slot, &mut v);
                }
            }
        }
        // Each group is sorted outward from its extreme, so its last entry is the
        // count'th lowest or highest.
        self.target_low = lowest[count - 1];
        self.target_high = highest[count - 1];
    }

    /// Where `v` sits in the measured range, 0.0-1.0. The range is at least `min_span` wide.
    fn normalize(&self, v: f32, min_span: f32) -> f32 {
        let span = (self.high - self.low).max(min_span);
        ((v - self.low) / span).clamp(0.0, 1.0)
    }
}

/// Band edge `i` of the MEL_BANDS + 2 points between BAND_LOW_HZ and BAND_HIGH_HZ,
/// each a constant ratio above the last.
fn edge_hz(i: usize) -> f32 {
    BAND_LOW_HZ * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(i as f32 / (MEL_BANDS + 1) as f32)
}

/// Fraction bits of the fixed-point coefficients and signal (the chip has no FPU).
/// a1 reaches -1.937, and a full-scale tone on a band center drives the state to 1.004.
const COEF_Q: u32 = 30;
const SIG_Q: u32 = 28;

/// One second-order bandpass filter: passes frequencies near its center.
struct Biquad {
    b0: i32,
    a1: i32,
    a2: i32,
    /// Filter state, carried across samples and frames.
    s1: i32,
    s2: i32,
}

impl Biquad {
    /// Constant-peak-gain bandpass (Audio EQ Cookbook) centered on `center_hz` with
    /// a -3 dB width of `bandwidth_hz`, normalized so the a0 coefficient is 1.
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
        // Direct form II transposed. A bandpass has b1 = 0 and b2 = -b0, so the one
        // product b0*x serves both feed-forward taps.
        let bx = Self::mul(self.b0, x);
        let y = bx + self.s1;
        self.s1 = self.s2 - Self::mul(self.a1, y);
        self.s2 = -bx - Self::mul(self.a2, y);
        y
    }
}

/// The 24 bandpass filters and one energy sum per band.
struct BandBank {
    filters: [Biquad; MEL_BANDS],
    /// Sum of squared filter output per band since the last `take_energies`.
    energy: [i64; MEL_BANDS],
    /// Samples added to `energy`.
    count: u32,
}

impl BandBank {
    /// Band m is centered on edge m+1 and spans edges m..m+2, so neighbors overlap.
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

    /// Mean-square energy per band since the last call. Clears the sums but not the
    /// filter state, so the bands stay continuous across frames.
    fn take_energies(&mut self) -> [f32; MEL_BANDS] {
        let scale = self.count.max(1) as f32 * (1i64 << SIG_Q) as f32;
        let out = std::array::from_fn(|m| self.energy[m] as f32 / scale);
        self.energy = [0; MEL_BANDS];
        self.count = 0;
        out
    }
}

/// Computes the band levels and the overall level from raw audio samples.
pub struct MelProcessor {
    bank: BandBank,

    /// Smoothed RMS behind the overall level.
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

    /// Last frame's untilted band levels in dB, for the console readout.
    last_db: [f32; MEL_BANDS],

    /// Reference for `level_norm`, fed the broadband dBFS each frame.
    level_ref: RangeTracker<LEVEL_WINDOW_FRAMES>,

    /// Per-band smoothed output, 0.0-1.0.
    band_smooth: [f32; MEL_BANDS],
}

impl MelProcessor {
    pub fn new() -> Self {
        Self {
            bank: BandBank::new(),
            smoothed_rms: 0.0,
            band_ref: RangeTracker::new(),
            band_own: std::array::from_fn(|_| RangeTracker::new()),
            tilt: {
                let per_band =
                    (BAND_HIGH_HZ / BAND_LOW_HZ).log2() / (MEL_BANDS + 1) as f32;
                std::array::from_fn(|m| {
                    TILT_DB_PER_OCTAVE * (m as f32 - TILT_PIVOT_BAND) * per_band
                })
            },
            band_lo: 0.0,
            band_hi: 0.0,
            last_db: [0.0; MEL_BANDS],
            level_ref: RangeTracker::new(),
            band_smooth: [0.0; MEL_BANDS],
        }
    }

    /// Quietest and loudest band of the last frame, in dB, for the console readout.
    pub fn band_reference(&self) -> (f32, f32) { (self.band_lo, self.band_hi) }

    /// Last frame's band levels in dB, before the tilt, for the console readout.
    pub fn band_levels(&self) -> &[f32; MEL_BANDS] { &self.last_db }

    /// Live level reference (low, high) in dBFS, for the console readout.
    pub fn level_reference(&self) -> (f32, f32) { (self.level_ref.low, self.level_ref.high) }

    /// Turn one frame of samples into a `MelFrame`.
    pub fn process(&mut self, samples: &[i16; FFT_SIZE]) -> MelFrame {
        // Overall level: RMS of the raw samples, scaled to -1.0..1.0.
        let rms = (samples
            .iter()
            .map(|&s| (s as f32 / 32768.0).powi(2))
            .sum::<f32>()
            / FFT_SIZE as f32)
            .sqrt();
        // Rise fast and fall slowly, so the level does not flicker between beats.
        self.smoothed_rms = follow(self.smoothed_rms, rms, LEVEL_ATTACK, LEVEL_DECAY);
        // RMS is already normalized to full scale, so dBFS needs no calibration.
        let dbfs = if self.smoothed_rms > 0.0 {
            20.0 * self.smoothed_rms.log10()
        } else {
            LEVEL_DB_FLOOR
        };
        let level = level_to_wire(dbfs);
        self.level_ref.push(dbfs);
        let level_norm = norm_to_wire(self.level_ref.normalize(dbfs, LEVEL_MIN_SPAN_DB));

        for &s in samples.iter() {
            // i16 -> Q28: s/32768 scaled by 2^28 is exactly s << 13.
            self.bank.push((s as i32) << (SIG_Q - 15));
        }
        // Band energy in dB. The 1e-10 guards log(0) and floors silence at -100 dB.
        let mut band_db = [0f32; MEL_BANDS];
        for (b, e) in band_db.iter_mut().zip(self.bank.take_energies()) {
            *b = 10.0 * (e + 1e-10).log10();
        }

        // Each band is scaled between the shared reference, which keeps relative loudness,
        // and its own, which keeps it moving. The references see tilted levels; the gate
        // sees true ones.
        self.band_lo = band_db.iter().copied().fold(f32::MAX, f32::min);
        self.band_hi = band_db.iter().copied().fold(f32::MIN, f32::max);
        let tilted_peak = band_db
            .iter()
            .zip(self.tilt.iter())
            .fold(f32::MIN, |m, (&db, &t)| m.max(db + t));
        self.band_ref.push(tilted_peak);

        // Total rise across the bands, from the raw dB, where a struck drum still has a
        // sharp edge.
        let mut flux_db = 0.0f32;
        for (db, prev) in band_db.iter().zip(self.last_db.iter()) {
            let step = db - prev;
            if step > 0.0 {
                flux_db += step;
            }
        }
        let flux = norm_to_wire(flux_db / FLUX_FULL_DB);
        let bass = level_to_wire(band_db[..BASS_BANDS].iter().sum::<f32>() / BASS_BANDS as f32);
        self.last_db = band_db;
        let shared_high = self.band_ref.high;
        let shared_low = shared_high - BAND_VISIBLE_RANGE_DB;

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
            *sm = follow(*sm, shaped, BAND_ATTACK, BAND_DECAY);
            bands[m] = norm_to_wire(*sm);
        }

        MelFrame { bands, level, level_norm, flux, bass }
    }
}
