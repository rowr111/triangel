mod audio;
mod mel;
mod uart_out;

use audio::{ActiveAudio, AudioSource};
use uart_out::UartOut;

fn main() -> ! {
    log_server::init_wait().unwrap();
    log::set_max_level(log::LevelFilter::Info);
    log::info!("ear starting, PID {}", xous::process::id());

    let hal = bao1x_hal_service::Hal::new();
    hal.set_preemption(true);

    let mut audio    = ActiveAudio::new();
    let mut uart_out = UartOut::new();

    log::info!("ear ready");

    // Send raw RMS level directly - same math as the Python bar display.
    // level = min(rms * 10, 1.0)  =>  100% bar = full fill.
    loop {
        let samples = audio.read_frame();
        let rms = (samples.iter().map(|&s| (s as f32 / 32768.0).powi(2)).sum::<f32>()
            / samples.len() as f32).sqrt();
        uart_out.send_level(rms * 1.8); // tune: higher = more sensitive
    }
}
