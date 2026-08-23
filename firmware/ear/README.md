# triangel - ear firmware

Audio processor firmware for the triangel fixture. Runs on a Baochip-1x under [Xous OS](https://betrusted.io/xous-book/) (DABAO dev board used during development). See [`../README.md`](../README.md) for the overall system and physical fixture description.

## What this is

The **ear** chip captures audio, computes a mel filterbank, and streams the result to the **eye** chip over UART at ~30 fps. The eye chip uses the mel band data to drive sound-reactive LED patterns.

Pipeline per frame (~32 ms):

```
mic (I2S)  ──►  512-sample frame  ──►  24 bandpass filters  ──►  24 mel bands  ──►  UART TX  ──►  eye
```

The 24 mel bands span 31–8000 Hz on a perceptual (mel) scale — lower bands are narrower in Hz to match how hearing works. This is the same structure as a hardware spectrum analyzer display, and it is built the same way: one bandpass filter per band, squaring and averaging each filter's output over the frame to get that band's energy. No FFT is involved, so there is no windowing and no frame-boundary reset — the filters run continuously. Each band value is u16 (0–65535), log-compressed and normalized against an adaptive ceiling that tracks the loudest band. An activity flag is also sent, set when sustained RMS energy exceeds a calibrated threshold; the eye uses this for Auto sound mode switching.

## Hardware

| Thing | Detail |
|---|---|
| Chip | Baochip-1x - 350 MHz VexRiscv RV32-IMAC, 2 MB SRAM, 4 MB ReRAM |
| Microphone | ICS43434 MEMS mic (JLCPCB C5656610), I2S slave |
| Eye link | Pin 15 PB14 (UART2 TX) → eye pin 16 PB13 (UART2 RX), single wire + GND, 921600 baud |

## Audio configuration

| Setting | Value | Notes |
|---|---|---|
| Sample rate | 16 kHz | Nyquist limit for 8 kHz mel ceiling |
| Bit depth | 24-bit | ICS43434 native; top 16 bits used |
| Channels | Mono | IS_SELECT pin tied low on PCB = left channel |
| Frame size | 512 samples | ~32 ms per frame, ~31 fps |

## Project structure

```
src/
+-- main.rs       - entry point; audio capture → mel → UART loop
+-- audio.rs      - AudioSource trait + I2sAudio: ICS43434 mic over I2S
+-- mel.rs        - MelProcessor: 24 mel-spaced bandpass filters, log + normalize
+-- uart_out.rs   - UartOut: encodes MelFrame and transmits to eye over UART
+-- diag.rs       - USB-serial output: boot stages, heartbeat, command input
+-- console.rs    - mic diagnostic commands
```

Wire protocol types shared between ear and eye live in [`../shared/`](../shared/) (`triangel-shared` crate).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). Audio comes from the ICS43434 MEMS mic over I2S.

## Mic diagnostic console

The ear has no log console - UART2 carries the audio link to the eye, so the board runs the `gdb-stub` kernel and the log server has no UART left to print on. Everything the firmware reports goes over **USB CDC serial** instead.

The board presents two serial ports: the bootloader's and the running application's. The console is on the application's.

Until the first keystroke the board prints a liveness line every 2 s naming the startup stage it reached and how many bytes it has received, so a stalled boot says where it stopped. Commands are a single character and act on the keystroke - no Enter, because terminals disagree on whether that sends CR, LF, or both.

| Key | What it does |
|---|---|
| `c` | Record 3 s silently, then print the level per 100 ms. The test that answers whether the mic hears you |
| `f` | Measure a quiet second against a noisy one, per octave band. About 30 dB more sensitive than `c`, and works with any sound |
| `r` | Hex-dump the raw 24-bit words after settling |
| `s` | Count samples for 1 s and compare against the expected 48000 Hz |
| `t` | 1 s of statistics: min, max, DC offset, RMS |
| `m` | Live level meter for 15 s |
| `?` | Help |

Commands run on the audio thread between frames, so the eye stops receiving mel frames while one is in progress - up to 15 s for `m`, 3 s or less for the rest. The eye decays to silence after 200 ms without a frame and recovers on its own; because its Auto-mode arm and release times are both 30 s, even the longest command will not flip it out of sound-reactive mode.
