mod audio;
mod mel;
mod uart_out;

use audio::{ActiveAudio, AudioSource};
use mel::MelProcessor;
use uart_out::UartOut;

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("ear starting, PID {}", xous::process::id());

    let hal = bao1x_hal_service::Hal::new();
    hal.set_preemption(true);

    // Audio source is selected by feature flag:
    //   uart-audio  -> UartAudio: receives PCM packets over USB serial from ear_sim.py
    //   (default)   -> I2sAudio: reads from ICS43434 MEMS mic via I2S hardware
    let mut uart_out = UartOut::new();

    log::info!("ear ready");

    // Smooth ramp test: fill 0→full over ~5s then full→0 over ~5s, repeat.
    // Uses send count for timing (10 000 sends × ~510 µs/send ≈ 5 s) so it
    // works regardless of whether std::thread::sleep is functional.
    // TODO: remove once audio reactivity is confirmed visually.
    {
        use triangel_shared::mel::MelFrame;
        // 10 000 sends × ~510µs/send ≈ 5s per ramp direction at 1Mbaud
        const RAMP_SENDS: u32 = 10_000;
        let mut n: u32 = 0;
        loop {
            let t     = (n % (2 * RAMP_SENDS)) as f32 / RAMP_SENDS as f32;
            let level = if t < 1.0 { t } else { 2.0 - t };
            let val   = (level * 65535.0) as u16;
            uart_out.send(&MelFrame {
                bands:    [val; triangel_shared::mel::MEL_BANDS],
                activity: level > 0.05,
            });
            n = n.wrapping_add(1);
        }
    }

    // Real loop (unreachable during test):
    #[allow(unreachable_code, unused_variables)]
    {
        let mut audio = ActiveAudio::new();
        let mut mel   = MelProcessor::new();
        loop {
            let samples = audio.read_frame();
            let frame   = mel.process(&samples);
            uart_out.send(&frame);
        }
    }
}
