//! Sound-reactive tuning. Build and flash the eye after changing all of these 
//! except the last (ear) section.

/// Beat detection shared by Firework, Tiles and Spiderweb.
pub mod beat {
    /// Bands checked for bass under a hit: 0-2 is 40-100 Hz.
    pub const KICK_BANDS: usize = 3;
    /// How much bass there has to be for a hit to count. Keeps hi-hats out.
    pub const LOW_MIN: f32 = 0.15;
    /// A beat fires when the rise across all bands reaches this many times its recent
    /// average. Lower fires more often.
    pub const FLUX_TRIGGER: f32 = 1.9;
    /// It fires again only after dropping back under this.
    pub const FLUX_RELEASE: f32 = 1.3;
    /// How quickly that recent average follows. Lower is steadier.
    pub const FLUX_AVG_RATE: f32 = 0.02;
    /// Shortest gap between beats.
    pub const BEAT_REFRACTORY_MS: u32 = 100;
    /// Gaps accepted as a beat when measuring tempo.
    pub const BEAT_MIN_MS: f32 = 250.0;
    pub const BEAT_MAX_MS: f32 = 1200.0;
}

/// Per-band onsets, used by Raindrop's trigger and Spectrum's hits.
pub mod onset {
    /// How fast a band's quick envelope follows it.
    pub const BAND_FAST: f32 = 0.6;
    /// How fast its slow baseline follows. Lower makes hits stand out more.
    pub const BAND_SLOW: f32 = 0.02;
    /// Scales the gap between the two into 0-1.
    pub const RISE_GAIN: f32 = 1.5;
}

/// Drop detection: the bass leaving for a while, then coming back.
pub mod drop_detect {
    /// Below this the bass is room noise and no breakdown can start.
    pub const BASS_MIN_DB: f32 = -75.0;
    /// How far below normal the bass has to fall, and for how long, to be a breakdown.
    pub const BREAKDOWN_DB: f32 = 10.0;
    pub const BREAKDOWN_MS: u32 = 4_000;
    /// The drop fires when the bass comes back within this of normal.
    pub const DROP_DB: f32 = 6.0;
    /// Give up on a breakdown after this long with no drop.
    pub const BREAKDOWN_MAX_MS: u32 = 90_000;
    /// Breakdown length at which the build reaches full.
    pub const BUILD_FULL_MS: f32 = 16_000.0;
}

/// Overall loudness, and when Auto switches to the sound patterns.
pub mod level {
    /// Loudness that counts as music playing, in dBFS.
    pub const ACTIVITY_LOUD_DBFS: f32 = -45.0;
    /// Loud time before Auto switches to sound patterns, and quiet time before it switches back.
    pub const ACTIVITY_ARM_MS: f32 = 30_000.0;
    pub const ACTIVITY_RELEASE_MS: f32 = 30_000.0;
    /// dBFS range mapped onto 0-1 for the absolute level.
    pub const RENDER_DB_FLOOR: f32 = -70.0;
    pub const RENDER_DB_CEIL: f32 = -20.0;
}

pub mod spectrum {
    /// Brightness in a quiet room.
    pub const QUIET_FLOOR: f32 = 0.20;
    /// Overall brightness. Higher clips loud bands to full.
    pub const GAIN: f32 = 1.25;
    /// How much a hit brightens a band, and how much it whitens it.
    pub const HIT_GAIN: f32 = 0.9;
    pub const HIT_WHITE: f32 = 0.65;
    /// Depth and speed of the slow ripple.
    pub const SWIRL_DEPTH: f32 = 0.18;
    pub const SWIRL_PERIOD_MS: u32 = 12_000;
    /// How far the ripple moves colors along the palette.
    pub const RIPPLE_SHIFT: f32 = 0.10;
    /// Time for colors to flow from the center to the edge and back.
    pub const FLOW_PERIOD_MS: u32 = 14_000;
    /// Time on each palette, and how long the next one takes to sweep out.
    pub const PALETTE_MS: u32 = 25_000;
    pub const PALETTE_FADE_MS: u32 = 5_000;
    /// Softness of the edge where a new palette sweeps in, in bands.
    pub const FRONT_EDGE: f32 = 3.0;
}

/// Raindrop. Has its own trigger, separate from `beat`.
pub mod raindrop {
    /// Bands its trigger listens to: 0-2 is 40-100 Hz.
    pub const KICK_BANDS: usize = 3;
    /// Fires when a bass band's onset reaches this. Lower fires more often.
    pub const TRIGGER: f32 = 0.20;
    /// It fires again only after dropping back under this.
    pub const RELEASE: f32 = 0.10;
    /// Shortest gap between drops.
    pub const REFRACTORY_MS: u32 = 120;
    /// Drops per beat: at least BURST_MIN, plus up to BURST_EXTRA on a hard beat.
    pub const BURST_MIN: usize = 2;
    pub const BURST_EXTRA: f32 = 2.0;
    /// Ripple speed in mm per ms, and how far one travels before it is gone.
    pub const RIPPLE_SPEED: f32 = 0.22;
    pub const RIPPLE_MAX_MM: f32 = 560.0;
    /// Brightness of a soft drop, and how much more a hard one gets.
    pub const GAIN_BASE: f32 = 1.2;
    pub const GAIN_HIT: f32 = 3.0;
    /// Ring spacing when no tempo has been found, in mm.
    pub const SPACING_MIN_MM: f32 = 24.0;
    pub const SPACING_MAX_MM: f32 = 42.0;
    /// Rings per drop.
    pub const RINGS_MIN: f32 = 2.0;
    pub const RINGS_MAX: f32 = 5.0;
    /// Size a drop starts at, in mm.
    pub const START_RADIUS_MM: f32 = 24.0;
    /// How far a ripple travels while still white, and how much brighter it is then.
    pub const WHITE_MM: f32 = 180.0;
    pub const HIT_BOOST: f32 = 4.5;
}

pub mod firework {
    /// Sparks per burst: at least BURST_MIN, plus up to BURST_EXTRA on a hard beat.
    pub const BURST_MIN: usize = 8;
    pub const BURST_EXTRA: f32 = 8.0;
    /// Bursts on a drop, and how far apart they are kept, in mm. Keep DROP_BURSTS times
    /// (BURST_MIN + BURST_EXTRA) well under 192, the most sparks that can fly at once.
    pub const DROP_BURSTS: usize = 7;
    pub const DROP_SPACING_MM: f32 = 110.0;
    /// Spark speed range in mm per ms, and how long a spark lasts.
    pub const SPEED_MIN: f32 = 0.08;
    pub const SPEED_MAX: f32 = 0.20;
    pub const SPARK_LIFE_MS: f32 = 1500.0;
    /// How fast sparks droop.
    pub const GRAVITY: f32 = 0.00005;
    /// Spark size at the start and end of its life, in mm.
    pub const SPARK_START_MM: f32 = 44.0;
    pub const SPARK_END_MM: f32 = 18.0;
    /// Share of a spark's life spent white, and how much brighter it is then.
    pub const WHITE_FRAC: f32 = 0.22;
    pub const HIT_BOOST: f32 = 2.2;
    /// White background level and how much it moves. 0 turns it off.
    pub const WASH_BASE: f32 = 0.0;
    pub const WASH_DEPTH: f32 = 0.0;
}

pub mod tiles {
    /// How long a lit tile takes to fade, and the delay between rings as a hit spreads.
    pub const FADE_MS: f32 = 800.0;
    pub const RING_STEP_MS: u32 = 60;
    /// On a drop, how long every tile holds at full, then how long it fades.
    pub const DROP_HOLD_MS: f32 = 700.0;
    pub const DROP_FADE_MS: f32 = 2500.0;
    /// Share of a tile's life spent white, and how much brighter it is then.
    pub const WHITE_FRAC: f32 = 0.15;
    pub const HIT_BOOST: f32 = 1.5;
    /// Extra tiles a hard beat spreads to.
    pub const GROW_EXTRA: f32 = 8.0;
}

pub mod spiderweb {
    /// Resting glow, how much a beat lifts it, and how fast it settles.
    pub const BASE_LEVEL: f32 = 0.08;
    pub const PULSE_DEPTH: f32 = 1.5;
    pub const PULSE_MS: f32 = 300.0;
    /// How long a lit line stays white, and how much brighter it is then.
    pub const HEAD_MS: f32 = 90.0;
    pub const HEAD_BOOST: f32 = 1.5;
    /// How long a lit line takes to fade back to the glow.
    pub const FADE_MS: f32 = 2800.0;
    /// Extra lines a hard beat lights.
    pub const EXTRA_LINES: f32 = 2.0;
    /// On a drop, how long every line holds at full, then how long it fades.
    pub const DROP_HOLD_MS: f32 = 700.0;
    pub const DROP_FADE_MS: f32 = 2500.0;
}

/// Flashing the ear after changing anything here.
pub mod ear {
    /// History behind each reference, in 32 ms frames: all bands, each band, and level.
    pub const BAND_WINDOW_FRAMES: usize = 94;
    pub const BAND_OWN_WINDOW_FRAMES: usize = 48;
    pub const LEVEL_WINDOW_FRAMES: usize = 234;
    /// How fast a reference moves toward louder, and back toward quieter.
    pub const REF_RISE: f32 = 0.5;
    pub const REF_FALL: f32 = 0.06;
    /// dB below the loudest band that shows as dark.
    pub const BAND_VISIBLE_RANGE_DB: f32 = 26.0;
    /// How much each band is scaled to its own range: 0 keeps true loudness, 1 evens them out.
    pub const PER_BAND_MIX: f32 = 0.45;
    /// Smallest range a band, and the level, get stretched over.
    pub const BAND_MIN_SPAN_DB: f32 = 8.0;
    pub const LEVEL_MIN_SPAN_DB: f32 = 6.0;
    /// Lift per octave toward the treble, and the band it pivots on.
    pub const TILT_DB_PER_OCTAVE: f32 = 2.0;
    pub const TILT_PIVOT_BAND: f32 = 8.0;
    /// Quietest a band can be and still show, and the range it fades in over.
    pub const GATE_FLOOR_DB: f32 = -100.0;
    pub const GATE_KNEE_DB: f32 = 10.0;
    /// Contrast curve for the bands. Higher darkens the middle.
    pub const POWER_LAW: f32 = 1.4;
    /// How fast a band rises and falls.
    pub const BAND_ATTACK: f32 = 0.6;
    pub const BAND_DECAY: f32 = 0.25;
    /// Rise across all bands that reads as full flux.
    pub const FLUX_FULL_DB: f32 = 120.0;
}
