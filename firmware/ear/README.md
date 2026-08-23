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
```

Wire protocol types shared between ear and eye live in [`../shared/`](../shared/) (`triangel-shared` crate).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). Audio comes from the ICS43434 MEMS mic over I2S.
