mod audio;
mod input;
mod led;
mod patterns;
mod setlist;

#[cfg(feature = "bringup")]
mod cmds;
#[cfg(feature = "bringup")]
mod ctap;
#[cfg(feature = "bringup")]
mod repl;
#[cfg(feature = "bringup")]
mod shell;

#[cfg(feature = "bringup")]
use cmds::*;

use led::map::LED_MAP;
use setlist::SetlistManager;

const TARGET_FRAME_MS: u64 = 1000 / 30; // ~33 ms → 30 fps

fn main() -> ! {
    #[cfg(not(feature = "previewer"))]
    log_server::init_wait().unwrap();
    #[cfg(not(feature = "previewer"))]
    log::set_max_level(log::LevelFilter::Info);
    #[cfg(not(feature = "previewer"))]
    log::info!("eye starting, PID {}", xous::process::id());

    let tt = ticktimer::Ticktimer::new().unwrap();

    let hal = bao1x_hal_service::Hal::new();
    hal.set_preemption(true);

    // Hardware / previewer output
    let mut led_out = led::LedOutput::new();

    // Audio receiver (spawns background UART listener thread)
    let audio = audio::AudioReceiver::new();

    // Input event queue (spawns button + IR threads)
    let event_queue = input::new_queue();
    input::spawn(event_queue.clone());

    // Setlist manager owns pattern cycling, brightness, sound mode
    let mut setlist = SetlistManager::new(tt.elapsed_ms() as u32);

    // Frame buffer — reused every frame to avoid allocation
    let mut frame = [[0u8; 3]; led::map::LED_COUNT];

    #[cfg(feature = "bringup")]
    shell::start_shell();

    #[cfg(not(feature = "previewer"))]
    log::info!("entering render loop");

    // Absolute next-frame deadline — prevents timing drift across frames.
    let mut next_frame = tt.elapsed_ms();

    loop {
        next_frame += TARGET_FRAME_MS;
        let frame_start = tt.elapsed_ms();

        // Determine sound-reactive mode
        let sound_level = audio.smoothed_level();
        let sound_active = setlist.sound_active(audio.is_active());

        // Drain input events and apply to setlist
        input::apply_events(&event_queue, &mut setlist, sound_active);

        // Advance cycling timer
        setlist.tick(frame_start as u32, sound_active);

        // Render current pattern into frame buffer
        setlist.current_pattern(sound_active).render(&LED_MAP, frame_start as u32, sound_level, &mut frame);

        // Apply global brightness
        let brightness = setlist.brightness;
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
        }
    }
}
