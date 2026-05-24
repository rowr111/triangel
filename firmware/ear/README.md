# triangel - ear firmware

Audio processor firmware for the triangel fixture. Runs on a [Baochip DABAO](https://www.crowdsupply.com/baochip/dabao) under [Xous OS](https://betrusted.io/xous-book/). See [`../README.md`](../README.md) for the overall system and physical fixture description.

## What this is

The **ear** chip captures audio, computes a mel filterbank, and streams the result to the **eye** chip over UART at ~30 fps. The eye chip uses the mel band data to drive sound-reactive LED patterns.

Pipeline per frame (~32 ms):

```
mic (I2S)  ──►  512-sample frame  ──►  Hann window + FFT  ──►  24 mel bands  ──►  UART TX  ──►  eye
```

The 24 mel bands span 40–8000 Hz on a perceptual (mel) scale — lower bands are narrower in Hz to match how hearing works. This is the same structure as a hardware spectrum analyzer display. Each band value is u16 (0–65535), log-compressed and min-max normalized per frame. An activity flag is also sent, set when sustained RMS energy exceeds a calibrated threshold; the eye uses this for Auto sound mode switching.

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
+-- audio.rs      - AudioSource trait + UartAudio (uart-audio feature) + I2sAudio (production)
+-- mel.rs        - MelProcessor: Hann window, FFT, triangular mel filters, log + normalize
+-- uart_out.rs   - UartOut: encodes MelFrame and transmits to eye over UART
```

Wire protocol types shared between ear and eye live in [`../shared/`](../shared/) (`triangel-shared` crate).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). The `uart-audio` feature swaps the audio source from the real I2S mic to USB serial input from `ear_sim.py` on a desktop:

| Feature | Audio source | Use case |
|---|---|---|
| _(none)_ | ICS43434 MEMS mic via I2S | Production |
| `uart-audio` | USB serial from `ear_sim.py` | Development / testing without the mic |

Add `uart-audio` under **Extra Features** in the Baochip extension settings when developing without the assembled mic circuit.

## Desktop testing with ear_sim.py

`ear_sim.py` in [`../../triangel previewer/`](../../triangel%20previewer/) streams desktop audio or mic input to the ear board over USB serial, letting the full mel filterbank and UART output path run on real music without the ICS43434 mic being assembled. Build with the `uart-audio` feature and see the [previewer README](../../triangel%20previewer/README.md) for setup and usage.
