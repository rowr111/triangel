mod audio;
mod diag;
mod input;
mod led;
mod patterns;
mod pins;
mod setlist;

#[cfg(feature = "bringup")]
mod cmds;
#[cfg(feature = "bringup")]
mod repl;
#[cfg(feature = "bringup")]
mod shell;

#[cfg(feature = "bringup")]
use cmds::*;

use led::map::LED_MAP;
use setlist::SetlistManager;

const TARGET_FRAME_MS: u64 = 1000 / 30; // ~33 ms -> 30 fps

fn main() -> ! {
    // A panic anywhere (main or a thread) prints over USB serial before the
    // process dies; the USB server owns the message once sent, so it still
    // reaches the host. Without this, panics are invisible: the log console
    // is unavailable on this system build.
    std::panic::set_hook(Box::new(|info| {
        diag::Diag::new().line(&format!("PANIC: {}", info));
    }));

    // Diagnostics and heartbeat come up before anything that could block, so
    // a stalled boot still reports the stage it is stuck at over USB serial.
    let boot_diag = diag::Diag::new();
    diag::stage(&boot_diag, 0);
    diag::spawn_heartbeat();

    #[cfg(not(feature = "previewer"))]
    log_server::init_wait().unwrap();
    #[cfg(not(feature = "previewer"))]
    log::set_max_level(log::LevelFilter::Info);
    #[cfg(not(feature = "previewer"))]
    log::info!("eye starting, PID {}", xous::process::id());
    diag::stage(&boot_diag, 1);

    let tt = ticktimer::Ticktimer::new().unwrap();

    let hal = bao1x_hal_service::Hal::new();
    hal.set_preemption(true);

    // Hardware / previewer output
    let mut led_out = led::LedOutput::new();
    diag::stage(&boot_diag, 2);

    // Audio receiver (continuous DMA into the UART's IFRAM ring; no interrupts, no threads)
    let mut audio = audio::AudioReceiver::new();
    diag::stage(&boot_diag, 3);

    // Input event queue (spawns button + IR threads)
    let event_queue = input::new_queue();
    input::spawn(event_queue.clone());
    diag::stage(&boot_diag, 4);

    // Setlist manager owns pattern cycling, brightness, sound mode
    let mut setlist = SetlistManager::new(tt.elapsed_ms() as u32);

    // Frame buffer - reused every frame to avoid allocation
    let mut frame = [[0u8; 3]; led::map::LED_COUNT];

    #[cfg(feature = "bringup")]
    shell::start_shell();

    #[cfg(not(feature = "previewer"))]
    log::info!("entering render loop");
    diag::stage(&boot_diag, 5);

    // Absolute next-frame deadline - prevents timing drift across frames.
    let mut next_frame = tt.elapsed_ms();

    loop {
        next_frame += TARGET_FRAME_MS;
        let frame_start = tt.elapsed_ms();

        // Ingest whatever the RX DMA ring collected; decay toward zero when ear is silent
        audio.update(frame_start as u32);

        let sound_level = audio.smoothed_level();

        // Drain input events and apply to setlist; it derives sound_active per event.
        input::apply_events(&event_queue, &mut setlist, frame_start as u32, audio.is_active());

        // Determine sound-reactive mode after events: they may have changed it this frame.
        let sound_active = setlist.sound_active(audio.is_active());

        // Advance cycling timer
        setlist.tick(frame_start as u32, sound_active);

        // Render current pattern (compositing any in-flight transition) into frame buffer
        setlist.render(&LED_MAP, frame_start as u32, sound_level, sound_active, &mut frame);

        // Apply global brightness
        let brightness = setlist.brightness();
        if brightness < 1.0 {
            for led in frame.iter_mut() {
                led[0] = (led[0] as f32 * brightness) as u8;
                led[1] = (led[1] as f32 * brightness) as u8;
                led[2] = (led[2] as f32 * brightness) as u8;
            }
        }

        // Send to LEDs (WS2812 or previewer serial depending on feature flag)
        led_out.send_frame(&frame);

        // Sleep until the next scheduled frame deadline.
        let now = tt.elapsed_ms();
        if now < next_frame {
            tt.sleep_ms((next_frame - now) as usize).ok();
        } else {
            // Overran the deadline; resync so we don't sprint through a burst of catch-up frames.
            next_frame = now;
        }
    }
}
