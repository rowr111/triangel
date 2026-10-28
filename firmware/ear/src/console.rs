//! Mic diagnostics over USB serial. A console thread reads keystrokes and the audio loop
//! runs each command between frames.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::audio::{I2sAudio, BCLK_HZ, FRAME_PERIOD_MS, RAW_RATE_HZ, SAMPLE_RATE_HZ};
use crate::diag::{self, Diag};
use crate::pins::{MIC_BCLK_BIO_PIN, MIC_WS_BIO_PIN};
use triangel_shared::frame::{level_to_norm, BAND_COUNT, LEVEL_DB_FLOOR};

/// Raw FIFO words shown by `raw`, and how many are printed per line.
const RAW_DUMP_WORDS: usize = 32;
const RAW_DUMP_PER_LINE: usize = 8;
/// Measurement window for `samples` and `tally`.
const MEASURE_MS: u64 = 1000;
/// How long `meter` runs.
const METER_MS: u64 = 15_000;
/// How many windows `capture` records.
const CAPTURE_WINDOWS: usize = 30;
/// One window of `meter` or `capture`.
const WINDOW_MS: usize = 100;
const WINDOW_SAMPLES: usize = RAW_RATE_HZ as usize * WINDOW_MS / 1000;
/// Octave band centers for `frequencies`. A Q of 1.41 makes each filter about an octave wide.
const OCTAVE_COUNT: usize = 6;
const OCTAVE_HZ: [f32; OCTAVE_COUNT] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0];
const OCTAVE_Q: f32 = 1.41;
/// `frequencies` records one second at the pipeline's rate, then filters it.
const ANALYZE_SAMPLES: usize = SAMPLE_RATE_HZ as usize;
/// A band this far above its own quiet reading counts as heard.
const OCTAVE_RISE_DB: f32 = 6.0;
/// Pause after the prompt, so there is time to start making noise.
const READY_MS: usize = 1500;
/// Samples discarded before a measurement, so it starts on fresh, settled audio.
const SETTLE_SAMPLES: usize = RAW_RATE_HZ as usize / 5; // 200 ms
const METER_SETTLE_SAMPLES: usize = RAW_RATE_HZ as usize / 20; // 50 ms, between windows
/// Samples read between clock checks; the clock is a syscall, too slow per sample.
const SAMPLE_BLOCK: usize = 256;
/// Full scale for a sign-extended 24-bit sample.
const FULL_SCALE_RAW: f64 = 8_388_608.0;
/// The 24 bits of a raw sample.
const SAMPLE_BITS: u32 = 0x00ff_ffff;
/// The ICS43434 reads -26 dBFS at 94 dB SPL, so sound pressure is roughly dBFS + 120.
const DBFS_TO_SPL: f32 = 120.0;
/// Width of the meter's bar. Its left edge is LEVEL_DB_FLOOR.
const BAR_WIDTH: usize = 40;

/// How often the console thread checks whether the audio loop has run its command.
const COMMAND_POLL_MS: usize = 20;

/// Messages more than one command prints.
const NO_SAMPLES: &str = "no samples: the BIO core is not pushing anything";
const STARVED: &str = "recording starved - the BIO stopped pushing partway";

/// The console's commands, each run by one key.
#[derive(Clone, Copy)]
enum Command {
    Capture,
    Frequencies,
    Raw,
    Samples,
    Tally,
    Meter,
    Perf,
    Norm,
    Help,
}

impl Command {
    fn from_key(key: char) -> Option<Command> {
        Some(match key {
            'c' => Command::Capture,
            'f' => Command::Frequencies,
            'r' => Command::Raw,
            's' => Command::Samples,
            't' => Command::Tally,
            'm' => Command::Meter,
            'p' => Command::Perf,
            'n' => Command::Norm,
            '?' | 'h' => Command::Help,
            _ => return None,
        })
    }
}

/// An f32 that both threads can read and write, kept as its bits.
struct AtomicF32(AtomicU32);

impl AtomicF32 {
    const fn new() -> Self {
        AtomicF32(AtomicU32::new(0))
    }

    fn load(&self) -> f32 {
        f32::from_bits(self.0.load(Ordering::Relaxed))
    }

    fn store(&self, value: f32) {
        self.0.store(value.to_bits(), Ordering::Relaxed);
    }
}

/// One window's RMS and absolute peak, in raw sample units.
#[derive(Clone, Copy)]
struct Window {
    rms: f64,
    peak: i32,
}

/// The key waiting to be run, or 0. The audio thread runs it: the mic is not `Send`.
static PENDING: AtomicU32 = AtomicU32::new(0);

/// Latest filterbank cost in microseconds, published by the audio loop for `perf`.
static BANDS_US: AtomicU32 = AtomicU32::new(0);

/// The level reference, the level and the band levels, for `norm`.
static LEVEL_REF_LOW: AtomicF32 = AtomicF32::new();
static LEVEL_REF_HIGH: AtomicF32 = AtomicF32::new();
static LEVEL_DBFS: AtomicF32 = AtomicF32::new();
static LEVEL_NORM: AtomicF32 = AtomicF32::new();
static BAND_DB: [AtomicF32; BAND_COUNT] = [const { AtomicF32::new() }; BAND_COUNT];

/// Publish the filterbank's cost per frame for `perf`.
pub fn record_bands_time(us: u32) {
    BANDS_US.store(us, Ordering::Relaxed);
}

/// Publish the band levels for `norm`.
pub fn record_bands(db: &[f32; BAND_COUNT]) {
    for (slot, &value) in BAND_DB.iter().zip(db.iter()) {
        slot.store(value);
    }
}

/// Publish the level reference and the level for `norm`.
pub fn record_level(level_ref: (f32, f32), level_dbfs: f32, level_norm: f32) {
    LEVEL_REF_LOW.store(level_ref.0);
    LEVEL_REF_HIGH.store(level_ref.1);
    LEVEL_DBFS.store(level_dbfs);
    LEVEL_NORM.store(level_norm);
}

/// Start the console thread. It blocks waiting for a keystroke.
pub fn spawn() {
    std::thread::spawn(|| {
        let d = Diag::new();
        let tt = ticktimer::Ticktimer::new().unwrap();
        loop {
            let key = d.command();
            // First keystroke silences the heartbeat so it cannot break up output.
            diag::quiet();
            match Command::from_key(key) {
                // These need no mic, so answer here without interrupting audio.
                Some(Command::Help) => help(&d),
                Some(Command::Perf) => perf(&d),
                Some(Command::Norm) => norm(&d),
                _ => {
                    PENDING.store(key as u32, Ordering::Release);
                    while PENDING.load(Ordering::Acquire) != 0 {
                        tt.sleep_ms(COMMAND_POLL_MS).ok();
                    }
                }
            }
        }
    });
}

/// Run the waiting command, if any. Called by the audio loop between frames.
pub fn service(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    let pending = PENDING.load(Ordering::Acquire);
    if pending == 0 {
        return;
    }
    let key = char::from_u32(pending).unwrap_or('?');
    match Command::from_key(key) {
        Some(Command::Raw) => raw(d, mic),
        Some(Command::Samples) => samples(d, tt, mic),
        Some(Command::Tally) => tally(d, tt, mic),
        Some(Command::Meter) => meter(d, tt, mic),
        Some(Command::Capture) => capture(d, mic),
        Some(Command::Frequencies) => frequencies(d, tt, mic),
        _ => d.line(&format!("unknown command '{}' - try ? (help)", key)),
    }
    // Releases the console thread to print its prompt again.
    PENDING.store(0, Ordering::Release);
}

pub fn help(d: &Diag) {
    d.line(&format!(
        "  c  capture      record {} s silently, then print the level per {} ms - the",
        CAPTURE_WINDOWS * WINDOW_MS / 1000,
        WINDOW_MS
    ));
    d.line("                  test that answers whether the mic hears you");
    d.line("  f  frequencies  measure a quiet second, then a noisy one, and compare them");
    d.line("                  per octave band - about 30 dB more sensitive than");
    d.line("                  c (capture), and works with any sound");
    d.line("  r  raw          hex-dump the raw 24-bit words after settling");
    d.line(&format!(
        "  s  samples      count samples for {} s, compare against the expected {} Hz",
        MEASURE_MS / 1000,
        RAW_RATE_HZ
    ));
    d.line(&format!(
        "  t  tally        {} s of statistics: min, max, DC offset, RMS",
        MEASURE_MS / 1000
    ));
    d.line(&format!(
        "  m  meter        live level meter for {} s",
        METER_MS / 1000
    ));
    d.line("  p  perf         filterbank time per frame");
    d.line("  n  norm         the band range, the level reference and the level it scales");
    d.line("  ?  help         this help");
}

/// Report the band range and the level reference.
fn norm(d: &Diag) {
    let band_db: [f32; BAND_COUNT] = std::array::from_fn(|band| BAND_DB[band].load());
    let band_low = band_db.iter().copied().fold(f32::MAX, f32::min);
    let band_high = band_db.iter().copied().fold(f32::MIN, f32::max);
    let (level_low, level_high) = (LEVEL_REF_LOW.load(), LEVEL_REF_HIGH.load());
    d.line(&format!(
        "band range {:>7.1} .. {:>7.1} dB, span {:.1}",
        band_low,
        band_high,
        band_high - band_low
    ));
    d.line(&format!(
        "level ref  {:>7.1} .. {:>7.1} dBFS, span {:.1}",
        level_low,
        level_high,
        level_high - level_low
    ));
    d.line(&format!(
        "level now  {:>7.1} dBFS -> norm {:.2}",
        LEVEL_DBFS.load(),
        LEVEL_NORM.load()
    ));
    let mut line = String::from("band dB  ");
    for (band, db) in band_db.iter().enumerate() {
        if band == BAND_COUNT / 2 {
            d.line(&line);
            line = String::from("         ");
        }
        line.push_str(&format!("{:5.0}", db));
    }
    d.line(&line);
}

/// Report what the filterbank costs per frame.
fn perf(d: &Diag) {
    let us = BANDS_US.load(Ordering::Relaxed);
    if us == 0 {
        d.line("no timing yet - the first batch of frames has not finished");
        return;
    }
    let ms = us as f32 / 1000.0;
    d.line(&format!(
        "bands: {:.2} ms to process each {} ms frame ({:.0}%)",
        ms,
        FRAME_PERIOD_MS,
        ms / FRAME_PERIOD_MS as f32 * 100.0
    ));
}

/// Drain and discard `samples`. False if nothing is arriving.
fn settle(mic: &mut I2sAudio, samples: usize) -> bool {
    mic.flush();
    mic.discard(samples)
}

/// Reads samples for MEASURE_MS, checking the clock every SAMPLE_BLOCK, and hands each to
/// `f`. False if a read timed out partway.
fn measure(tt: &ticktimer::Ticktimer, mic: &mut I2sAudio, mut f: impl FnMut(i32)) -> bool {
    let start = tt.elapsed_ms();
    loop {
        for _ in 0..SAMPLE_BLOCK {
            // One timeout is enough: a drained FIFO never stays empty that long.
            let Some(sample) = mic.try_read_sample() else {
                return false;
            };
            f(sample);
        }
        if tt.elapsed_ms() - start >= MEASURE_MS {
            return true;
        }
    }
}

/// Hex-dump the raw 24-bit words the BIO pushes.
fn raw(d: &Diag, mic: &mut I2sAudio) {
    d.line(&format!("FIFO0 level before read: {}", mic.fifo_level()));
    if !settle(mic, SETTLE_SAMPLES) {
        d.line("nothing arriving while settling");
        return;
    }

    let mut line = String::new();
    for i in 0..RAW_DUMP_WORDS {
        // All 32 bits: anything set above bit 23 means the BIO is pushing more than the sample.
        match mic.try_read_raw() {
            Some(word) => line.push_str(&format!("{:08x} ", word)),
            None => line.push_str("-------- "),
        }
        if i % RAW_DUMP_PER_LINE == RAW_DUMP_PER_LINE - 1 {
            d.line(line.trim_end());
            line.clear();
        }
    }
    // A partial last line, if the two constants do not divide evenly.
    if !line.is_empty() {
        d.line(line.trim_end());
    }

    d.line("all 000000 = data line low or mic unpowered; all ffffff = line high or");
    d.line("floating; one value repeating = latching problem; small values changing");
    d.line("sign = working");
}

/// Count raw samples for a second.
fn samples(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    mic.flush();
    let start = tt.elapsed_ms();
    let mut count = 0u64;
    let starved = !measure(tt, mic, |_| count += 1);

    let elapsed = (tt.elapsed_ms() - start).max(1);
    let rate = count * 1000 / elapsed;
    let expected = RAW_RATE_HZ as u64;
    d.line(&format!(
        "{} samples in {} ms = {} Hz (expected {})",
        count, elapsed, rate, expected
    ));

    if count == 0 {
        d.line(&format!(
            "nothing arriving at all: check that BCLK (PB{}) and WS (PB{}) are really",
            MIC_BCLK_BIO_PIN, MIC_WS_BIO_PIN
        ));
        d.line(&format!(
            "toggling - {:.3} MHz and {} kHz respectively",
            BCLK_HZ as f32 / 1e6,
            RAW_RATE_HZ / 1000
        ));
    } else if starved {
        d.line("a read timed out on an empty FIFO - the BIO stopped pushing partway");
    } else if rate * 4 < expected * 3 || rate * 3 > expected * 4 {
        d.line("rate is off by a large factor: suspect the BIO quantum divider,");
        d.line("not the mic");
    }
}

/// A second of statistics, raw and as the i16 the pipeline sees.
fn tally(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }

    let (mut min, mut max) = (i32::MAX, i32::MIN);
    let (mut sum, mut sum_squares, mut count) = (0i64, 0i64, 0i64);
    // Bits that were ever set, and bits that were always set.
    let (mut any_set, mut all_set) = (0u32, u32::MAX);
    let starved = !measure(tt, mic, |sample| {
        let bits = sample as u32 & SAMPLE_BITS;
        any_set |= bits;
        all_set &= bits;
        min = min.min(sample);
        max = max.max(sample);
        sum += sample as i64;
        sum_squares += (sample as i64) * (sample as i64);
        count += 1;
    });

    if count == 0 {
        d.line(NO_SAMPLES);
        return;
    }

    let (mean, rms) = mean_and_rms(sum, sum_squares, count);
    let (min, max) = (min as i64, max as i64);

    if starved {
        d.line("the BIO stopped pushing partway - this covers less than the full second");
    }
    d.line(&format!("{} samples", count));
    d.line(&format!(
        "raw 24-bit: min {} max {} p-p {} dc {} rms {:.1}",
        min,
        max,
        max - min,
        mean,
        rms
    ));
    let rms16 = rms / 256.0;
    d.line(&format!(
        "as i16    : min {} max {} p-p {} dc {} rms {:.2}",
        min >> 8,
        max >> 8,
        (max >> 8) - (min >> 8),
        mean >> 8,
        rms16
    ));
    let db = dbfs(rms, FULL_SCALE_RAW);
    d.line(&format!("level     : {}", bar(db)));
    d.line(&format!(
        "          : roughly {:.0} dB SPL",
        db + DBFS_TO_SPL
    ));

    // The ICS43434 leaves bit 0 unused, so one stuck low bit is expected. Bit 23 always set
    // while bit 22 changes means the capture window opens a cycle early.
    d.line(&format!(
        "bits      : any-set {:08x} all-set {:08x}",
        any_set, all_set
    ));
    if any_set == 0 {
        d.line("every bit was zero for the whole window - the data line never went high");
    } else if any_set.trailing_zeros() > 1 {
        d.line(&format!(
            "the low {} bits never went high - only bit 0 should be stuck",
            any_set.trailing_zeros()
        ));
    }
    if all_set & SAMPLE_BITS == SAMPLE_BITS {
        d.line("every bit was high in every sample - nothing is driving the data line.");
        d.line("Check the wiring, and that the mic's channel select is tied to GND so it");
        d.line("transmits in the half of the frame we read.");
    } else if all_set & ((1 << 23) | (1 << 22)) == 1 << 23 {
        d.line("bit 23 was set in every sample - the capture window is opening too early");
        d.line("and reading the idle data line instead of the mic's first bit");
    }

    if rms16 < 1.0 {
        d.line("the i16 RMS rounds to zero: whatever is on the wire would be crushed by");
        d.line("the >> 8 in read_frame even if the raw numbers look alive");
    }
}

/// Live level, each window on fresh samples.
fn meter(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    d.line(&format!(
        "live level for {} s - clap, talk, play music",
        METER_MS / 1000
    ));
    d.line("(if this looks unresponsive, use c (capture) - it records without stopping");
    d.line("to print)");
    let end = tt.elapsed_ms() + METER_MS;

    while tt.elapsed_ms() < end {
        let window = if settle(mic, METER_SETTLE_SAMPLES) {
            window_rms(mic, WINDOW_SAMPLES)
        } else {
            None
        };
        let Some(Window { rms, .. }) = window else {
            d.line("no samples - stopping");
            return;
        };
        d.line(&bar(dbfs(rms, FULL_SCALE_RAW)));
    }
}

/// Record silently, then report, so printing cannot drop samples.
fn capture(d: &Diag, mic: &mut I2sAudio) {
    d.line(&format!(
        "recording {} s silently - make noise NOW (clap, talk, music)",
        CAPTURE_WINDOWS * WINDOW_SAMPLES / RAW_RATE_HZ as usize
    ));

    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }

    let mut recorded = [Window { rms: 0.0, peak: 0 }; CAPTURE_WINDOWS];
    for slot in recorded.iter_mut() {
        match window_rms(mic, WINDOW_SAMPLES) {
            Some(window) => *slot = window,
            None => {
                d.line(STARVED);
                return;
            }
        }
    }

    d.line("window   rms      peak   level");
    let (mut quietest, mut loudest) = (f64::MAX, 0f64);
    for (i, window) in recorded.iter().enumerate() {
        quietest = quietest.min(window.rms);
        loudest = loudest.max(window.rms);
        d.line(&format!(
            "{:4} {:8.0} {:9} {}",
            i,
            window.rms,
            window.peak,
            bar(dbfs(window.rms, FULL_SCALE_RAW))
        ));
    }

    // All zeros is no mic, not a quiet one, and would give a meaningless range below.
    if loudest < 1.0 {
        d.line("every window was exactly zero - nothing is driving the data line at all.");
        d.line("This is a wiring or channel-select problem, not a quiet room. Run t (tally).");
        return;
    }

    let loud_db = dbfs(loudest, FULL_SCALE_RAW);
    let range_db = loud_db - dbfs(quietest.max(1.0), FULL_SCALE_RAW);
    d.line(&format!(
        "quietest {:.0}, loudest {:.0}, range {:.1} dB",
        quietest, loudest, range_db
    ));
    d.line(&format!(
        "loudest window is {:.1} dBFS = roughly {:.0} dB SPL",
        loud_db,
        loud_db + DBFS_TO_SPL
    ));

    // A mic that heard a clap or a voice peaks tens of dB over its own floor.
    if range_db < 20.0 {
        d.line("that is not sound. A clap or raised voice should peak 20-40 dB above the");
        d.line("floor; this moved less than 20 dB, which is what a noise floor does on its");
        d.line("own. Run f (frequencies) to see whether any sound gets through attenuated.");
    } else {
        d.line("the level tracked the noise - the mic hears");
    }
}

/// A quiet second against a noisy one, per octave band. More sensitive than `capture`.
fn frequencies(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }
    let mut buf = Vec::with_capacity(ANALYZE_SAMPLES);

    d.line("stay quiet - measuring the floor for 1 s");
    if !record(d, mic, &mut buf) {
        return;
    }
    let quiet = octave_levels(&buf);

    d.line("now MAKE NOISE - whistle, clap, talk - starting in a moment");
    tt.sleep_ms(READY_MS).ok();
    if !settle(mic, METER_SETTLE_SAMPLES) || !record(d, mic, &mut buf) {
        return;
    }
    let loud = octave_levels(&buf);

    d.line("  band     quiet      loud      diff");
    let mut heard = false;
    for (i, &hz) in OCTAVE_HZ.iter().enumerate() {
        // Zero has no dB value; dbfs()'s floor would read as a rise.
        if quiet[i] <= 0.0 || loud[i] <= 0.0 {
            d.line(&format!(
                "{:6.0}   no reading - this band produced no energy",
                hz
            ));
            continue;
        }
        let quiet_db = dbfs(quiet[i], 1.0);
        let loud_db = dbfs(loud[i], 1.0);
        let diff = loud_db - quiet_db;
        heard |= diff >= OCTAVE_RISE_DB;
        d.line(&format!(
            "{:6.0} {:9.1} {:9.1} {:9.1} dB{}",
            hz,
            quiet_db,
            loud_db,
            diff,
            if diff >= OCTAVE_RISE_DB { "  <-" } else { "" }
        ));
    }

    if heard {
        d.line("a band rose while you made noise, so sound is reaching the transducer.");
        d.line("However far down it is, the path is attenuated rather than dead.");
    } else {
        d.line("no band rose. Nothing you made reached the transducer at any pitch, and");
        d.line("this measurement is roughly 30 dB more sensitive than c (capture).");
    }
}

/// Fill the buffer with one second at the pipeline's rate.
fn record(d: &Diag, mic: &mut I2sAudio, buf: &mut Vec<i16>) -> bool {
    buf.clear();
    for _ in 0..ANALYZE_SAMPLES {
        let Some(sample) = mic.read_decimated() else {
            d.line(STARVED);
            return false;
        };
        buf.push(sample);
    }
    true
}

/// RMS per octave band.
fn octave_levels(buf: &[i16]) -> [f64; OCTAVE_COUNT] {
    let mean = buf.iter().map(|&sample| sample as f64).sum::<f64>() / buf.len() as f64;
    let mut filters = OCTAVE_HZ.map(Bandpass::new);
    for &sample in buf {
        let x = ((sample as f64 - mean) / 32768.0) as f32;
        for filter in filters.iter_mut() {
            filter.feed(x);
        }
    }
    filters.map(|filter| filter.rms(buf.len()))
}

/// One octave-wide bandpass filter that sums the energy passing it.
struct Bandpass {
    b0: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
    energy: f64,
}

impl Bandpass {
    fn new(hz: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * hz / SAMPLE_RATE_HZ as f32;
        let alpha = w0.sin() / (2.0 * OCTAVE_Q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
            energy: 0.0,
        }
    }

    fn feed(&mut self, x: f32) {
        // b1 is zero for a bandpass, so the numerator is just b0 * (x - x2).
        let y = self.b0 * (x - self.x2) - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        self.energy += (y * y) as f64;
    }

    fn rms(&self, count: usize) -> f64 {
        (self.energy / count as f64).sqrt()
    }
}

/// RMS and absolute peak over one window of raw samples. None if the BIO stopped.
fn window_rms(mic: &mut I2sAudio, samples: usize) -> Option<Window> {
    let (mut sum, mut sum_squares) = (0i64, 0i64);
    let mut peak = 0i32;
    for _ in 0..samples {
        let sample = mic.try_read_sample()?;
        sum += sample as i64;
        sum_squares += (sample as i64) * (sample as i64);
        peak = peak.max(sample.saturating_abs());
    }
    let (_, rms) = mean_and_rms(sum, sum_squares, samples as i64);
    Some(Window { rms, peak })
}

/// The mean of `count` samples, and their RMS about that mean.
fn mean_and_rms(sum: i64, sum_squares: i64, count: i64) -> (i64, f64) {
    let mean = sum as f64 / count as f64;
    (
        sum / count,
        (sum_squares as f64 / count as f64 - mean * mean)
            .max(0.0)
            .sqrt(),
    )
}

/// Level in dBFS for an RMS expressed in units of the given full scale.
fn dbfs(rms: f64, full_scale: f64) -> f32 {
    if rms <= 0.0 {
        LEVEL_DB_FLOOR
    } else {
        (20.0 * (rms / full_scale).log10()) as f32
    }
}

/// A dBFS bar with the number after it.
fn bar(db: f32) -> String {
    let filled = (level_to_norm(db).clamp(0.0, 1.0) * BAR_WIDTH as f32) as usize;

    let mut s = String::with_capacity(BAR_WIDTH + 24);
    s.push('[');
    for i in 0..BAR_WIDTH {
        s.push(if i < filled { '#' } else { ' ' });
    }
    s.push_str(&format!("] {:6.1} dB", db));
    s
}
