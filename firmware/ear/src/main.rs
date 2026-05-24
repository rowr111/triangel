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
    let mut audio = ActiveAudio::new();

    // Mel filterbank - builds FFT plan and filter weights once at startup
    let mut mel = MelProcessor::new();

    // UART output to eye chip
    let mut uart_out = UartOut::new();

    log::info!("ear ready");

    // Main loop: read_frame() blocks until a complete 512-sample audio frame
    // arrives (~32 ms at 16 kHz), so the loop runs at ~31 fps naturally.
    // No explicit sleep or timer needed - the audio source sets the pace.
    loop {
        let samples = audio.read_frame();
        let frame = mel.process(&samples);
        uart_out.send(&frame);
    }
}
