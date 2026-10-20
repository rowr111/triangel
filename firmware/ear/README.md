# Triangel - ear firmware

Audio processor firmware for the Triangel. Runs on a Baochip-1x under [Xous OS](https://betrusted.io/xous-book/) (dabao dev board used during development). See [`../README.md`](../README.md) for the overall system and physical Triangel description.

## What this is

The **ear** chip captures audio, measures it in 24 frequency bands, and streams the result to the **eye** chip over UART.

Pipeline per frame (32 ms of sound):

```
mic (I2S, 48 kHz)  ->  2:1 decimate  ->  768-sample frame  ->  24 bandpass filters  ->  normalize  ->  UART TX  ->  eye
```

The 24 bands span 40 Hz to 12 kHz, each a constant ratio above the last (about a third of an octave), so every band covers the same musical interval. It is built like a hardware spectrum analyzer: one bandpass filter per band, squaring and averaging each filter's output over the frame. No FFT is involved, so there is no windowing, and each filter keeps its state from one frame to the next.

Each frame carries two kinds of value, on purpose:

- **Bands** are normalized against recent music, so they give spectral shape but never go dark in a quiet room.
- **Level** and **bass** are absolute dBFS, so they do. dB SPL is roughly dBFS + 120.

It also carries the level normalized against recent loudness, and the spectral flux the eye uses for beat detection. The frame layout is in [`../shared/README.md`](../shared/README.md).

## Hardware

| Thing | Detail |
|---|---|
| Chip | Baochip-1x - 350 MHz VexRiscv RV32-IMAC, 2 MB SRAM, 4 MB ReRAM, no FPU |
| Microphone | ICS43434 MEMS mic (JLCPCB C5656610), I2S slave, clocked by a BIO core: PB1 = BCLK, PB2 = SD, PB3 = WS |
| Eye link | PB14 (UART2 TX, pin 15) -> eye PB13 (UART2 RX, pin 16), single wire + GND |

## Audio configuration

| Setting | Value | Notes |
|---|---|---|
| Mic sample rate | 48 kHz | Inside the ICS43434's high-performance range |
| Pipeline sample rate | 24 kHz | 2:1 box-average decimation; 12 kHz Nyquist |
| Bit depth | 24-bit | Top 16 bits used |
| Channels | Mono | Select pin tied low on PCB = left channel |
| Frame size | 768 samples | 32 ms of sound per frame. The sound that arrives while a frame is being processed is skipped, so a frame goes out about every 36 ms |

All of these derive from three constants at the top of [`src/audio.rs`](src/audio.rs).

Code shared with the eye (frame format, tuning) lives in [`../shared/`](../shared/).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`).

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
| `p` | Filterbank time per frame |
| `n` | The bands' range, the level reference, and the level it scales |
| `?` | Help |

`p`, `n` and `?` do not interrupt audio. The others stop frames to the eye while they run: up to 15 s for `m`, under 5 s for the rest. The eye fades to silence and recovers on its own, and no command is long enough to switch it out of sound-reactive mode.
