use std::sync::{Arc, Mutex};

pub use triangel_shared::mel::MEL_BANDS;
use triangel_shared::mel::{MelFrame, FRAME_LEN};

struct AudioState {
    mel:            [f32; MEL_BANDS],
    smoothed_level: f32,
    envelope:       f32,
    activity:       bool,
}

impl AudioState {
    fn new() -> Self {
        AudioState { mel: [0.0; MEL_BANDS], smoothed_level: 0.0, envelope: 0.0, activity: false }
    }
}

#[derive(Clone)]
pub struct AudioReceiver {
    state: Arc<Mutex<AudioState>>,
}

impl AudioReceiver {
    pub fn new() -> Self {
        let receiver = AudioReceiver { state: Arc::new(Mutex::new(AudioState::new())) };
        receiver.spawn_listener();
        receiver
    }

    /// Smoothed normalised sound level 0.0-1.0 with attack/decay envelope.
    pub fn smoothed_level(&self) -> f32 {
        self.state.lock().map(|s| s.smoothed_level).unwrap_or(0.0)
    }

    /// Raw mel band values, 24 bands, each 0.0-1.0.
    pub fn current_mel(&self) -> [f32; MEL_BANDS] {
        self.state.lock().map(|s| s.mel).unwrap_or([0.0; MEL_BANDS])
    }

    /// Activity flag set by the ear chip: true when sustained absolute sound energy
    /// exceeds the ear's calibrated threshold. Use this for Auto sound mode rather
    /// than smoothed_level, which is post-normalisation and not a reliable loudness indicator.
    pub fn is_active(&self) -> bool {
        self.state.lock().map(|s| s.activity).unwrap_or(false)
    }

    fn spawn_listener(&self) {
        let state = self.state.clone();
        std::thread::spawn(move || {
            listen_loop(state);
        });
    }
}

// Fast attack, slow decay - level jumps quickly on loud sounds and fades gradually.
const ATTACK: f32 = 0.25;
const DECAY:  f32 = 0.02;

fn apply_envelope(envelope: f32, new_level: f32) -> f32 {
    if new_level > envelope {
        envelope + ATTACK * (new_level - envelope)
    } else {
        (envelope - DECAY).max(new_level).max(0.0)
    }
}

fn listen_loop(state: Arc<Mutex<AudioState>>) {
    let tt = ticktimer::Ticktimer::new().unwrap();
    loop {
        // TODO: receive mel frames from ear chip over UART.
        // Frame format is defined in triangel-shared: MelFrame::decode(&buf).
        // Expected: FRAME_LEN (51) bytes at ~30 fps.
        // For now, leave state at zeros so the lighting app runs cleanly without the ear chip.
        tt.sleep_ms(16).ok();

        // Once UART is wired, parse the frame here and update state:
        //
        //   let mut buf = [0u8; FRAME_LEN];
        //   uart.read_exact(&mut buf).ok();
        //   if let Some(frame) = MelFrame::decode(&buf) {
        //       let mut s = state.lock().unwrap();
        //       for (i, &raw) in frame.bands.iter().enumerate() {
        //           s.mel[i] = raw as f32 / 65535.0;
        //       }
        //       let raw_level = s.mel.iter().copied().fold(0.0_f32, f32::max);
        //       s.envelope = apply_envelope(s.envelope, raw_level);
        //       s.smoothed_level = s.envelope;
        //       s.activity = frame.activity;
        //   }
        let _ = (state.clone(), apply_envelope, FRAME_LEN, MelFrame::decode); // keep reachable until wired
    }
}
