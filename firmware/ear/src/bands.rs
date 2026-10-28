//! Turns each frame of microphone samples into a `BandFrame`: the level in 24 bands from
//! 40 Hz to 12 kHz, the overall level, the flux and the bass.
//!
//! Band centers are a third of an octave apart, and each band covers two thirds. With
//! equal-width bands, or the mel scale, a kick drum near 50 Hz and a voice near 120 Hz
//! would share a band; here they are four bands apart.

use triangel_shared::follow;
use triangel_shared::frame::{level_to_wire, norm_to_wire, BandFrame, BAND_COUNT, LEVEL_DB_FLOOR};
use triangel_shared::tuning::ear::*;
use triangel_shared::tuning::BASS_BANDS;

use crate::audio::{FRAME_SAMPLES, SAMPLE_RATE_HZ};

// Fixed values; the ones meant for tuning are in `triangel_shared::tuning::ear`.

/// Sample rate of the frames read_frame delivers, in Hz.
const SAMPLE_RATE: f32 = SAMPLE_RATE_HZ as f32;

/// Lowest frequency covered. 40 Hz puts a kick drum's fundamental in the first bands.
const BAND_LOW_HZ: f32 = 40.0;

/// Highest frequency covered: half the sample rate.
const BAND_HIGH_HZ: f32 = SAMPLE_RATE / 2.0;

/// Samples filtered but not measured at the start of each frame, after the gap in the audio.
const SETTLE_SAMPLES: usize = 96;

const I16_FULL_SCALE: f32 = 32768.0;

// How fast the smoothed RMS behind the level rises and falls, per frame.
const LEVEL_ATTACK: f32 = 0.8;
const LEVEL_DECAY: f32 = 0.4;

/// Values kept at each end of the window. The reference is the last of them, so one
/// loud spike cannot move it.
const EXTREME_COUNT: usize = 5;

/// Frames between reference recomputations (about 0.3 s).
const REFRESH_FRAMES: u32 = 8;

/// Fraction bits of the fixed-point coefficients and signal (the chip has no FPU).
/// a1 reaches -1.994, and full-scale input drives a filter step to 2.54.
const COEF_Q: u32 = 30;
const SIG_Q: u32 = 28;

/// Low and high reference over the last `N` frames: the `EXTREME_COUNT`th lowest and
/// highest values in the window.
struct RangeTracker<const N: usize> {
    history: [f32; N],
    next: usize,
    /// Values pushed so far, capped at `N`.
    filled: usize,
    /// Frames since the last recompute.
    age: u32,
    /// What the window measures now; `low` and `high` follow it.
    target_low: f32,
    target_high: f32,
    low: f32,
    high: f32,
}

impl<const N: usize> RangeTracker<N> {
    fn new() -> Self {
        // age starts due, so the first push computes a reference immediately.
        Self {
            history: [0.0; N],
            next: 0,
            filled: 0,
            age: REFRESH_FRAMES,
            target_low: 0.0,
            target_high: 0.0,
            low: 0.0,
            high: 0.0,
        }
    }

    fn push(&mut self, value: f32) {
        self.history[self.next] = value;
        self.next = (self.next + 1) % N;
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
        let mut lowest = [f32::MAX; EXTREME_COUNT];
        let mut highest = [f32::MIN; EXTREME_COUNT];
        for &value in self.history[..self.filled].iter() {
            if value < lowest[count - 1] {
                offer(&mut lowest[..count], value, |a, b| a < b);
            }
            if value > highest[count - 1] {
                offer(&mut highest[..count], value, |a, b| a > b);
            }
        }
        // Each group is sorted outward from its extreme, so its last entry is the
        // count'th lowest or highest.
        self.target_low = lowest[count - 1];
        self.target_high = highest[count - 1];
    }

    /// Where `value` sits in the measured range, 0.0-1.0. The range is at least `min_span` wide.
    fn normalize(&self, value: f32, min_span: f32) -> f32 {
        let span = (self.high - self.low).max(min_span);
        ((value - self.low) / span).clamp(0.0, 1.0)
    }
}

/// Inserts `value` into the sorted `group` where it beats an entry; the last entry drops off.
fn offer(group: &mut [f32], mut value: f32, beats: impl Fn(f32, f32) -> bool) {
    for slot in group.iter_mut() {
        if beats(value, *slot) {
            std::mem::swap(slot, &mut value);
        }
    }
}

/// Band edge `i` of the BAND_COUNT + 2 points between BAND_LOW_HZ and BAND_HIGH_HZ,
/// each a constant ratio above the last.
fn edge_hz(i: usize) -> f32 {
    BAND_LOW_HZ * (BAND_HIGH_HZ / BAND_LOW_HZ).powf(i as f32 / (BAND_COUNT + 1) as f32)
}

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
    /// a -3 dB width of `octaves`, normalized so the a0 coefficient is 1.
    fn bandpass(center_hz: f32, octaves: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * center_hz / SAMPLE_RATE;
        // Audio EQ Cookbook alpha for a width in octaves, accurate up to half the sample rate.
        let alpha = w0.sin() * (std::f32::consts::LN_2 / 2.0 * octaves * w0 / w0.sin()).sinh();
        let a0 = 1.0 + alpha;
        let to_q30 = |v: f32| (v as f64 * (1i64 << COEF_Q) as f64).round() as i32;
        Self {
            b0: to_q30(alpha / a0),
            a1: to_q30(-2.0 * w0.cos() / a0),
            a2: to_q30((1.0 - alpha) / a0),
            s1: 0,
            s2: 0,
        }
    }

    /// Q30 coefficient times Q28 signal, back to Q28.
    #[inline]
    fn mul(coef: i32, sig: i32) -> i32 {
        ((coef as i64 * sig as i64) >> COEF_Q) as i32
    }

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
    filters: [Biquad; BAND_COUNT],
    /// Sum of squared filter output per band since the last `take_energies`.
    energy: [i64; BAND_COUNT],
    /// Samples added to `energy`.
    count: u32,
}

impl BandBank {
    /// Band n is centered on edge n+1 and spans edges n..n+2, so neighbors overlap.
    fn new() -> Self {
        let filters = std::array::from_fn(|band| {
            Biquad::bandpass(
                edge_hz(band + 1),
                (edge_hz(band + 2) / edge_hz(band)).log2(),
            )
        });
        Self {
            filters,
            energy: [0; BAND_COUNT],
            count: 0,
        }
    }

    /// Run one sample through every band without measuring it.
    #[inline]
    fn settle(&mut self, sample: i32) {
        for filter in self.filters.iter_mut() {
            filter.step(sample);
        }
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
    fn take_energies(&mut self) -> [f32; BAND_COUNT] {
        let scale = self.count.max(1) as f32 * (1i64 << SIG_Q) as f32;
        let out = std::array::from_fn(|band| self.energy[band] as f32 / scale);
        self.energy = [0; BAND_COUNT];
        self.count = 0;
        out
    }
}

/// Computes the band levels and the overall level from raw audio samples.
pub struct BandProcessor {
    bank: BandBank,
    /// Smoothed RMS behind the overall level.
    smoothed_rms: f32,
    /// Shared reference, fed the loudest band's dB each frame. Holds the bands in
    /// their true loudness order.
    band_ref: RangeTracker<BAND_WINDOW_FRAMES>,
    /// Each band's own reference, so a quiet band still moves rather than sitting flat.
    band_own: [RangeTracker<BAND_OWN_WINDOW_FRAMES>; BAND_COUNT],
    /// Per-band dB lift that cancels music's natural rolloff with frequency.
    tilt: [f32; BAND_COUNT],
    /// Quietest and loudest band of the last frame, for the console readout.
    band_low: f32,
    band_high: f32,
    /// Last frame's untilted band levels in dB, which flux is measured against.
    last_db: [f32; BAND_COUNT],
    /// Reference for `level_norm`, fed the broadband dBFS each frame.
    level_ref: RangeTracker<LEVEL_WINDOW_FRAMES>,
    /// Last frame's level in dBFS and against its reference, before wire rounding, for the
    /// console readout.
    level_dbfs: f32,
    level_norm: f32,
    /// Per-band smoothed output, 0.0-1.0.
    band_smooth: [f32; BAND_COUNT],
}

impl BandProcessor {
    pub fn new() -> Self {
        Self {
            bank: BandBank::new(),
            smoothed_rms: 0.0,
            band_ref: RangeTracker::new(),
            band_own: std::array::from_fn(|_| RangeTracker::new()),
            tilt: {
                let per_band = (BAND_HIGH_HZ / BAND_LOW_HZ).log2() / (BAND_COUNT + 1) as f32;
                std::array::from_fn(|band| {
                    TILT_DB_PER_OCTAVE * (band as f32 - TILT_PIVOT_BAND) * per_band
                })
            },
            band_low: 0.0,
            band_high: 0.0,
            last_db: [0.0; BAND_COUNT],
            level_ref: RangeTracker::new(),
            level_dbfs: LEVEL_DB_FLOOR,
            level_norm: 0.0,
            band_smooth: [0.0; BAND_COUNT],
        }
    }

    /// Quietest and loudest band of the last frame, in dB, for the console readout.
    pub fn band_range(&self) -> (f32, f32) {
        (self.band_low, self.band_high)
    }

    /// Last frame's band levels in dB, before the tilt, for the console readout.
    pub fn band_levels(&self) -> &[f32; BAND_COUNT] {
        &self.last_db
    }

    /// Live level reference (low, high) in dBFS, for the console readout.
    pub fn level_reference(&self) -> (f32, f32) {
        (self.level_ref.low, self.level_ref.high)
    }

    /// Last frame's level in dBFS and 0.0-1.0, before wire rounding, for the console readout.
    pub fn level(&self) -> (f32, f32) {
        (self.level_dbfs, self.level_norm)
    }

    /// Turn one frame of samples into a `BandFrame`.
    pub fn process(&mut self, samples: &[i16; FRAME_SAMPLES]) -> BandFrame {
        let (level, level_norm) = self.overall_level(samples);
        let band_db = self.run_filters(samples);
        let flux = self.flux(&band_db);
        let bass = level_to_wire(band_db[..BASS_BANDS].iter().sum::<f32>() / BASS_BANDS as f32);
        self.last_db = band_db;
        BandFrame {
            bands: self.shape_bands(&band_db),
            level,
            level_norm,
            flux,
            bass,
        }
    }

    /// The overall level, and the level against its reference, both encoded for the wire.
    fn overall_level(&mut self, samples: &[i16; FRAME_SAMPLES]) -> (u16, u16) {
        // RMS of the raw samples, scaled to -1.0..1.0.
        let squares: u64 = samples
            .iter()
            .map(|&sample| (sample as i32 * sample as i32) as u64)
            .sum();
        let rms =
            (squares as f32 / (FRAME_SAMPLES as f32 * I16_FULL_SCALE * I16_FULL_SCALE)).sqrt();
        // Rise fast and fall slowly, so the level does not flicker between beats.
        self.smoothed_rms = follow(self.smoothed_rms, rms, LEVEL_ATTACK, LEVEL_DECAY);
        // RMS is already normalized to full scale, so dBFS needs no calibration.
        let dbfs = if self.smoothed_rms > 0.0 {
            (20.0 * self.smoothed_rms.log10()).max(LEVEL_DB_FLOOR)
        } else {
            LEVEL_DB_FLOOR
        };
        self.level_ref.push(dbfs);
        self.level_dbfs = dbfs;
        self.level_norm = self.level_ref.normalize(dbfs, LEVEL_MIN_SPAN_DB);
        (level_to_wire(dbfs), norm_to_wire(self.level_norm))
    }

    /// Runs the frame through the filterbank and returns each band's level in dB.
    fn run_filters(&mut self, samples: &[i16; FRAME_SAMPLES]) -> [f32; BAND_COUNT] {
        // i16 -> Q28: s/32768 scaled by 2^28 is exactly s << 13.
        let to_q28 = |sample: i16| (sample as i32) << (SIG_Q - 15);
        let (settle, measured) = samples.split_at(SETTLE_SAMPLES);
        for &sample in settle {
            self.bank.settle(to_q28(sample));
        }
        for &sample in measured {
            self.bank.push(to_q28(sample));
        }
        // Band energy in dB. The 1e-10 guards log(0) and floors silence at -100 dB.
        self.bank
            .take_energies()
            .map(|energy| 10.0 * (energy + 1e-10).log10())
    }

    /// Total rise across the bands, from the raw dB, where a struck drum still has a
    /// sharp edge.
    fn flux(&self, band_db: &[f32; BAND_COUNT]) -> u16 {
        let mut flux_db = 0.0f32;
        for (db, prev) in band_db.iter().zip(self.last_db.iter()) {
            let step = db - prev;
            if step > 0.0 {
                flux_db += step;
            }
        }
        norm_to_wire(flux_db / FLUX_FULL_DB)
    }

    /// Each band is scaled between the shared reference, which keeps relative loudness,
    /// and its own, which keeps it moving. The references see tilted levels; the gate
    /// sees true ones.
    fn shape_bands(&mut self, band_db: &[f32; BAND_COUNT]) -> [u16; BAND_COUNT] {
        self.band_low = band_db.iter().copied().fold(f32::MAX, f32::min);
        self.band_high = band_db.iter().copied().fold(f32::MIN, f32::max);
        let tilted_peak = band_db
            .iter()
            .zip(self.tilt.iter())
            .fold(f32::MIN, |peak, (&db, &tilt)| peak.max(db + tilt));
        self.band_ref.push(tilted_peak);
        let shared_high = self.band_ref.high;
        let shared_low = shared_high - BAND_VISIBLE_RANGE_DB;

        let mut bands = [0u16; BAND_COUNT];
        for (band, &raw_db) in band_db.iter().enumerate() {
            let db = raw_db + self.tilt[band];
            let own = &mut self.band_own[band];
            own.push(db);
            let high = shared_high + (own.high - shared_high) * PER_BAND_MIX;
            let low = shared_low + (own.low - shared_low) * PER_BAND_MIX;
            let gate = ((raw_db - GATE_FLOOR_DB) / GATE_KNEE_DB).clamp(0.0, 1.0);
            let norm = ((db - low) / (high - low).max(BAND_MIN_SPAN_DB)).clamp(0.0, 1.0) * gate;
            let shaped = norm.powf(POWER_LAW);
            let smooth = &mut self.band_smooth[band];
            *smooth = follow(*smooth, shaped, BAND_ATTACK, BAND_DECAY);
            bands[band] = norm_to_wire(*smooth);
        }
        bands
    }
}
