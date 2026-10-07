//! Mic diagnostics over USB serial. A console thread reads keystrokes and the audio loop
//! runs each command between frames.

use std::sync::atomic::{AtomicU32, Ordering};

use crate::audio::FRAME_PERIOD_MS;

use crate::audio::{I2sAudio, RAW_RATE_HZ, SAMPLE_RATE_HZ};
use triangel_shared::frame::{BAND_COUNT, LEVEL_DB_FLOOR};
use crate::diag::{self, Diag};

/// Raw FIFO words shown by `r`, and how many are printed per line.
const RAW_DUMP_WORDS:    usize = 32;
const RAW_DUMP_PER_LINE: usize = 8;
/// Measurement window for `s` and `t`.
const MEASURE_MS: u64 = 1000;
/// How long `m` runs.
const METER_MS: u64 = 15_000;
/// How many windows `c` records.
const CAPTURE_WINDOWS: usize = 30;
/// One window of `m` or `c`: 100 ms.
const WINDOW_SAMPLES: usize = RAW_RATE_HZ as usize / 10;
/// Octave band centers for `f`. A Q of 1.41 makes each filter about an octave wide.
const BANDS:   usize        = 6;
const BAND_HZ: [f32; BANDS] = [125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0];
const BAND_Q:  f32          = 1.41;
/// `f` records one second at the pipeline's rate, then filters it.
const ANALYZE_SAMPLES: usize = SAMPLE_RATE_HZ as usize;
/// A band this far above its own quiet reading counts as heard.
const BAND_RISE_DB: f32 = 6.0;
/// Pause after the prompt, so there is time to start making noise.
const READY_MS: usize = 1500;
/// Samples discarded before a measurement, so it starts on fresh, settled audio.
const SETTLE_SAMPLES:       usize = RAW_RATE_HZ as usize / 5;  // 200 ms
const METER_SETTLE_SAMPLES: usize = RAW_RATE_HZ as usize / 20; // 50 ms, between windows
/// Samples read between clock checks. Reading the clock is a syscall, too slow to do per sample.
const SAMPLE_BLOCK: usize = 256;
/// Full scale for a sign-extended 24-bit sample.
const FULL_SCALE_RAW: f64 = 8_388_608.0;
/// The ICS43434 reads -26 dBFS at 94 dB SPL, so sound pressure is roughly dBFS + 120.
const DBFS_TO_SPL: f32 = 120.0;
/// Width of the meter's bar. Its left edge is LEVEL_DB_FLOOR.
const BAR_WIDTH: usize = 40;

/// How often the console thread checks whether the audio loop has run its command.
const COMMAND_POLL_MS: usize = 20;

/// Messages more than one command prints.
const NO_SAMPLES: &str = "no samples: the BIO core is not pushing anything";
const STARVED:    &str = "recording starved - the BIO stopped pushing partway";

/// The keystroke waiting to be run, or 0. The mic is not `Send`, so the console thread
/// hands the key to the audio thread and waits.
static PENDING: AtomicU32 = AtomicU32::new(0);

/// Latest filterbank cost in microseconds, published by the audio loop for `p`.
static BANDS_US: AtomicU32 = AtomicU32::new(0);

/// Publish the filterbank's cost per frame for `p`.
pub fn record_bands_time(us: u32) { BANDS_US.store(us, Ordering::Relaxed); }

/// Live normalization references and the level they scale, as f32 bits, for `n`.
static REF_BAND_LO: AtomicU32 = AtomicU32::new(0);
static REF_BAND_HI: AtomicU32 = AtomicU32::new(0);
static REF_LVL_LO:  AtomicU32 = AtomicU32::new(0);
static REF_LVL_HI:  AtomicU32 = AtomicU32::new(0);
static LAST_DBFS:   AtomicU32 = AtomicU32::new(0);
static LAST_NORM:   AtomicU32 = AtomicU32::new(0);
static BAND_DB: [AtomicU32; BAND_COUNT] = [const { AtomicU32::new(0) }; BAND_COUNT];

/// Publish the band levels for `n`.
pub fn record_bands(db: &[f32; BAND_COUNT]) {
    for (slot, &v) in BAND_DB.iter().zip(db.iter()) {
        slot.store(v.to_bits(), Ordering::Relaxed);
    }
}

/// Publish the references and the level for `n`.
pub fn record_refs(band: (f32, f32), level: (f32, f32), dbfs: f32, norm: f32) {
    REF_BAND_LO.store(band.0.to_bits(), Ordering::Relaxed);
    REF_BAND_HI.store(band.1.to_bits(), Ordering::Relaxed);
    REF_LVL_LO.store(level.0.to_bits(), Ordering::Relaxed);
    REF_LVL_HI.store(level.1.to_bits(), Ordering::Relaxed);
    LAST_DBFS.store(dbfs.to_bits(), Ordering::Relaxed);
    LAST_NORM.store(norm.to_bits(), Ordering::Relaxed);
}

/// Start the console thread. It blocks waiting for a keystroke.
pub fn spawn() {
    std::thread::spawn(|| {
        let d = Diag::new();
        let tt = ticktimer::Ticktimer::new().unwrap();
        loop {
            let cmd = d.command();
            // First keystroke silences the heartbeat so it cannot break up output.
            diag::quiet();
            match cmd {
                // These need no mic, so answer here without interrupting audio.
                '?' | 'h' => help(&d),
                'p' => perf(&d),
                'n' => references(&d),
                _ => {
                    PENDING.store(cmd as u32, Ordering::Release);
                    while PENDING.load(Ordering::Acquire) != 0 {
                        tt.sleep_ms(COMMAND_POLL_MS).ok();
                    }
                }
            }
        }
    });
}

/// Run the command the console thread has read, if any. Called by the audio loop between
/// frames, so the eye link goes quiet only while a command runs.
pub fn service(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    let pending = PENDING.load(Ordering::Acquire);
    if pending == 0 {
        return;
    }
    match char::from_u32(pending).unwrap_or('?') {
        'r' => raw_dump(d, mic),
        's' => rate_check(d, tt, mic),
        't' => stats(d, tt, mic),
        'm' => meter(d, tt, mic),
        'c' => capture(d, mic),
        'f' => spectrum(d, tt, mic),
        other => d.line(&format!("unknown command '{}' - try ?", other)),
    }
    // Releases the console thread to print its prompt again.
    PENDING.store(0, Ordering::Release);
}

pub fn help(d: &Diag) {
    d.line("  c  record 3 s silently, then print the level per 100 ms - the test that");
    d.line("     answers whether the mic hears you");
    d.line("  f  measure a quiet second, then a noisy one, and compare them per octave");
    d.line("     band - about 30 dB more sensitive than c, and works with any sound");
    d.line("  r  hex-dump the raw 24-bit words after settling");
    d.line(&format!("  s  count samples for 1 s, compare against the expected {} Hz",
        RAW_RATE_HZ));
    d.line("  t  1 s of statistics: min, max, DC offset, RMS");
    d.line("  m  live level meter for 15 s");
    d.line("  p  filterbank time per frame");
    d.line("  n  live low/high normalization references and the level they scale");
    d.line("  ?  this help");
}

/// Report the live normalization references.
fn references(d: &Diag) {
    let g = |a: &AtomicU32| f32::from_bits(a.load(Ordering::Relaxed));
    let (blo, bhi) = (g(&REF_BAND_LO), g(&REF_BAND_HI));
    let (llo, lhi) = (g(&REF_LVL_LO), g(&REF_LVL_HI));
    d.line(&format!("band  ref {:>7.1} .. {:>7.1} dB, span {:.1}", blo, bhi, bhi - blo));
    d.line(&format!("level ref {:>7.1} .. {:>7.1} dBFS, span {:.1}", llo, lhi, lhi - llo));
    d.line(&format!("level now {:>7.1} dBFS -> norm {:.2}", g(&LAST_DBFS), g(&LAST_NORM)));
    let mut line = String::from("band dB  ");
    for (m, slot) in BAND_DB.iter().enumerate() {
        if m == 12 {
            d.line(&line);
            line = String::from("         ");
        }
        line.push_str(&format!("{:5.0}", f32::from_bits(slot.load(Ordering::Relaxed))));
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
    for _ in 0..samples {
        if mic.try_read_raw().is_none() {
            return false;
        }
    }
    true
}

/// Hex-dump the raw 24-bit words the BIO pushes.
fn raw_dump(d: &Diag, mic: &mut I2sAudio) {
    d.line(&format!("FIFO0 level before read: {}", mic.fifo_level()));
    if !settle(mic, SETTLE_SAMPLES) {
        d.line("nothing arriving while settling");
        return;
    }

    let mut line = String::new();
    for i in 0..RAW_DUMP_WORDS {
        // All 32 bits: anything set above bit 23 means the BIO is pushing more than the sample.
        match mic.try_read_raw() {
            Some(w) => line.push_str(&format!("{:08x} ", w)),
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

/// Count raw samples for a second. A rate off by a clean factor points at the BIO clock
/// divider, not the mic.
fn rate_check(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    mic.flush();
    let start = tt.elapsed_ms();
    let mut count = 0u64;
    let mut starved = false;

    'measure: loop {
        for _ in 0..SAMPLE_BLOCK {
            // One timeout is conclusive: in a tight drain loop the FIFO never stays empty that long.
            if mic.try_read_raw().is_none() {
                starved = true;
                break 'measure;
            }
            count += 1;
        }
        if tt.elapsed_ms() - start >= MEASURE_MS {
            break;
        }
    }

    let elapsed = (tt.elapsed_ms() - start).max(1);
    let rate = count * 1000 / elapsed;
    let expected = RAW_RATE_HZ as u64;
    d.line(&format!("{} samples in {} ms = {} Hz (expected {})",
        count, elapsed, rate, expected));

    if starved {
        d.line("a read timed out on an empty FIFO - the BIO stopped pushing partway");
    }
    if count == 0 {
        d.line("nothing arriving at all: check the pin mux, and that BCLK (PB1) and");
        d.line("WS (PB3) are really toggling - 3.072 MHz and 48 kHz respectively");
    } else if rate * 4 < expected * 3 || rate * 3 > expected * 4 {
        d.line("rate is off by a large factor: suspect the BIO quantum divider,");
        d.line("not the mic");
    }
}

/// A second of statistics, raw and as the i16 the pipeline sees, so a signal too small to
/// survive the 24 to 16 bit conversion shows as such.
fn stats(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }

    let start = tt.elapsed_ms();
    let mut starved = false;
    let (mut min, mut max) = (i32::MAX, i32::MIN);
    let (mut sum, mut sumsq, mut n) = (0i64, 0i64, 0i64);
    // Bits that were ever set, and bits that were always set.
    let (mut any_set, mut all_set) = (0u32, u32::MAX);

    'measure: loop {
        for _ in 0..SAMPLE_BLOCK {
            let Some(s) = mic.try_read_sample() else {
                starved = true;
                break 'measure;
            };
            let bits = s as u32 & 0x00ff_ffff;
            any_set |= bits;
            all_set &= bits;
            min = min.min(s);
            max = max.max(s);
            sum += s as i64;
            sumsq += (s as i64) * (s as i64);
            n += 1;
        }
        if tt.elapsed_ms() - start >= MEASURE_MS {
            break;
        }
    }

    if n == 0 {
        d.line(NO_SAMPLES);
        return;
    }

    let (mean, rms) = mean_and_rms(sum, sumsq, n);
    let (min, max) = (min as i64, max as i64);

    if starved {
        d.line("the BIO stopped pushing partway - this covers less than the full second");
    }
    d.line(&format!("{} samples", n));
    d.line(&format!("raw 24-bit: min {} max {} p-p {} dc {} rms {:.1}",
        min, max, max - min, mean, rms));
    let rms16 = rms / 256.0;
    d.line(&format!("as i16    : min {} max {} p-p {} dc {} rms {:.2}",
        min >> 8, max >> 8, (max >> 8) - (min >> 8), mean >> 8, rms16));
    let db = dbfs(rms, FULL_SCALE_RAW);
    d.line(&format!("level     : {}", bar(db)));
    d.line(&format!("          : roughly {:.0} dB SPL", db + DBFS_TO_SPL));

    // The ICS43434 leaves bit 0 unused, so one stuck low bit is expected. Bit 23 always set
    // means the capture window opens a cycle early and reads the idle-high data line.
    d.line(&format!("bits      : any-set {:08x} all-set {:08x}", any_set, all_set));
    if any_set == 0 {
        d.line("every bit was zero for the whole window - the data line never went high");
    } else if any_set.trailing_zeros() > 1 {
        d.line(&format!("the low {} bits never went high - only bit 0 should be stuck",
            any_set.trailing_zeros()));
    }
    if all_set & 0x00ff_ffff == 0x00ff_ffff {
        d.line("every bit was high in every sample - nothing is driving the data line.");
        d.line("Check the wiring, and that the mic's channel select is tied to GND so it");
        d.line("transmits in the half of the frame we read.");
    } else if all_set & 0x0080_0000 != 0 {
        d.line("bit 23 was set in every sample - the capture window is opening too early");
        d.line("and reading the idle data line instead of the mic's first bit");
    }

    if rms16 < 1.0 {
        d.line("the i16 RMS rounds to zero: whatever is on the wire would be crushed by");
        d.line("the >> 8 in read_frame even if the raw numbers look alive");
    }
}

/// Live level. Each window settles briefly first, so it starts on fresh samples.
fn meter(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    d.line(&format!("live level for {} s - clap, talk, play music", METER_MS / 1000));
    d.line("(if this looks unresponsive, use c - it records without stopping to print)");
    let end = tt.elapsed_ms() + METER_MS;

    while tt.elapsed_ms() < end {
        let window = if settle(mic, METER_SETTLE_SAMPLES) { window_rms(mic, WINDOW_SAMPLES) } else { None };
        let Some((rms, _)) = window else {
            d.line("no samples - stopping");
            return;
        };
        d.line(&bar(dbfs(rms, FULL_SCALE_RAW)));
    }
}

/// Record silently, then report. With no output during recording the drain never pauses,
/// so no samples are dropped.
fn capture(d: &Diag, mic: &mut I2sAudio) {
    d.line(&format!("recording {} s silently - make noise NOW (clap, talk, music)",
        CAPTURE_WINDOWS * WINDOW_SAMPLES / RAW_RATE_HZ as usize));

    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }

    let mut recorded = [(0f64, 0i32); CAPTURE_WINDOWS];
    for slot in recorded.iter_mut() {
        match window_rms(mic, WINDOW_SAMPLES) {
            Some(w) => *slot = w,
            None => {
                d.line(STARVED);
                return;
            }
        }
    }

    d.line("window   rms      peak   level");
    let (mut quietest, mut loudest) = (f64::MAX, 0f64);
    for (i, &(rms, peak)) in recorded.iter().enumerate() {
        quietest = quietest.min(rms);
        loudest = loudest.max(rms);
        d.line(&format!("{:4} {:8.0} {:9} {}", i, rms, peak, bar(dbfs(rms, FULL_SCALE_RAW))));
    }

    // All zeros is no mic, not a quiet one, and would give a meaningless range below.
    if loudest < 1.0 {
        d.line("every window was exactly zero - nothing is driving the data line at all.");
        d.line("This is a wiring or channel-select problem, not a quiet room. Run t.");
        return;
    }

    let loud_db = dbfs(loudest, FULL_SCALE_RAW);
    let range_db = loud_db - dbfs(quietest.max(1.0), FULL_SCALE_RAW);
    d.line(&format!("quietest {:.0}, loudest {:.0}, range {:.1} dB", quietest, loudest, range_db));
    d.line(&format!("loudest window is {:.1} dBFS = roughly {:.0} dB SPL",
        loud_db, loud_db + DBFS_TO_SPL));

    // A mic that heard a clap or a voice peaks tens of dB over its own floor.
    if range_db < 20.0 {
        d.line("that is not sound. A clap or raised voice should peak 20-40 dB above the");
        d.line("floor; this moved less than 20 dB, which is what a noise floor does on its");
        d.line("own. Run f to see whether any sound is getting through attenuated.");
    } else {
        d.line("the level tracked the noise - the mic hears");
    }
}

/// A quiet second against a noisy one, compared per octave band. Noise in the other bands
/// does not mask the one the sound lands in, so this is more sensitive than `c`.
fn spectrum(d: &Diag, tt: &ticktimer::Ticktimer, mic: &mut I2sAudio) {
    if !settle(mic, SETTLE_SAMPLES) {
        d.line(NO_SAMPLES);
        return;
    }
    let mut buf = Vec::with_capacity(ANALYZE_SAMPLES);

    d.line("stay quiet - measuring the floor for 1 s");
    if !record(d, mic, &mut buf) {
        return;
    }
    let quiet = band_levels(&buf);

    d.line("now MAKE NOISE - whistle, clap, talk - starting in a moment");
    tt.sleep_ms(READY_MS).ok();
    if !settle(mic, METER_SETTLE_SAMPLES) || !record(d, mic, &mut buf) {
        return;
    }
    let loud = band_levels(&buf);

    d.line("  band     quiet      loud      diff");
    let mut heard = false;
    for (i, &hz) in BAND_HZ.iter().enumerate() {
        // Zero has no dB value; dbfs()'s floor would read as a rise.
        if quiet[i] <= 0.0 || loud[i] <= 0.0 {
            d.line(&format!("{:6.0}   no reading - this band produced no energy", hz));
            continue;
        }
        let q = dbfs(quiet[i], 1.0);
        let l = dbfs(loud[i], 1.0);
        let diff = l - q;
        heard |= diff >= BAND_RISE_DB;
        d.line(&format!("{:6.0} {:9.1} {:9.1} {:9.1} dB{}", hz, q, l, diff,
            if diff >= BAND_RISE_DB { "  <-" } else { "" }));
    }

    if heard {
        d.line("a band rose while you made noise, so sound is reaching the transducer.");
        d.line("However far down it is, the path is attenuated rather than dead.");
    } else {
        d.line("no band rose. Nothing you made reached the transducer at any pitch, and");
        d.line("this measurement is roughly 30 dB more sensitive than c.");
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
fn band_levels(buf: &[i16]) -> [f64; BANDS] {
    let mean = buf.iter().map(|&s| s as f64).sum::<f64>() / buf.len() as f64;
    let mut filters = BAND_HZ.map(Bandpass::new);
    for &s in buf {
        let x = ((s as f64 - mean) / 32768.0) as f32;
        for f in filters.iter_mut() {
            f.feed(x);
        }
    }
    let mut out = [0f64; BANDS];
    for (o, f) in out.iter_mut().zip(filters.iter()) {
        *o = f.rms(buf.len());
    }
    out
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
        let alpha = w0.sin() / (2.0 * BAND_Q);
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

    fn rms(&self, n: usize) -> f64 {
        (self.energy / n as f64).sqrt()
    }
}

/// RMS about the mean and absolute peak over one window of raw samples. None if
/// the BIO stopped pushing partway.
fn window_rms(mic: &mut I2sAudio, samples: usize) -> Option<(f64, i32)> {
    let (mut sum, mut sumsq) = (0i64, 0i64);
    let mut peak = 0i32;
    for _ in 0..samples {
        let s = mic.try_read_sample()?;
        sum += s as i64;
        sumsq += (s as i64) * (s as i64);
        peak = peak.max(s.saturating_abs());
    }
    let (_, rms) = mean_and_rms(sum, sumsq, samples as i64);
    Some((rms, peak))
}

/// The mean of `n` samples, and their RMS about that mean, so the mic's DC offset does not
/// inflate the reading.
fn mean_and_rms(sum: i64, sumsq: i64, n: i64) -> (i64, f64) {
    let mean = sum / n;
    (mean, ((sumsq / n - mean * mean).max(0) as f64).sqrt())
}

/// Level in dBFS for an RMS expressed in units of the given full scale.
fn dbfs(rms: f64, full_scale: f64) -> f32 {
    if rms <= 0.0 { LEVEL_DB_FLOOR } else { (20.0 * (rms / full_scale).log10()) as f32 }
}

/// A dBFS bar with the number after it.
fn bar(db: f32) -> String {
    let filled = (((db - LEVEL_DB_FLOOR) / -LEVEL_DB_FLOOR).clamp(0.0, 1.0) * BAR_WIDTH as f32) as usize;

    let mut s = String::with_capacity(BAR_WIDTH + 24);
    s.push('[');
    for i in 0..BAR_WIDTH {
        s.push(if i < filled { '#' } else { ' ' });
    }
    s.push_str(&format!("] {:6.1} dB", db));
    s
}
