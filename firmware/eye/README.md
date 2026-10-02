# triangel - eye firmware

LED controller firmware for the triangel fixture. Runs on a Baochip-1x under [Xous OS](https://betrusted.io/xous-book/) (DABAO dev board used during development). See [`../README.md`](../README.md) for the overall system and physical fixture description.

## What this is

The **eye** chip drives 600 WS2812 LEDs across 25 triangle PCBs. It manages:

- A 30 fps render loop driving two LED chains in parallel, one BIO co-processor core each
- Two pattern setlists (ambient and sound-reactive), stepping every 3 minutes with a transition between patterns
- D-pad + 3-position switch for brightness, pattern stepping, hold, and sound mode
- IR remote (same functions as the physical controls)
- Audio frames from the **ear** chip over UART, from which it detects beats, drops and per-band onsets

## Hardware

| Thing | Detail |
|---|---|
| Chip | Baochip-1x - 350 MHz VexRiscv RV32-IMAC, 2 MB SRAM, 4 MB ReRAM, no FPU |
| BIO cores | 4x PicoRV - two for WS2812 bit-timing, one for IR capture |
| LED output | Two WS2812B chains: PB4 (288 LEDs) and PB5 (312 LEDs) |
| Ear link | PB13 (UART2 RX) |
| Controls | D-pad (5 buttons) + 3-position switch + IR receiver (PC8) |

All pin assignments are in [`src/pins.rs`](src/pins.rs).

## Project structure

```
src/
+-- main.rs      - entry point; 30 fps render loop
+-- setlist.rs   - the two setlists, cycling, transitions, brightness, sound mode
+-- audio.rs     - receives ear frames; beat, drop and activity detection
+-- pins.rs      - pin assignments
+-- diag.rs      - boot stages and heartbeat over USB serial
+-- led/
|   +-- mod.rs          - LedOutput: the WS2812 chains, or the USB previewer
|   +-- ws2812_pair.rs  - two-chain WS2812 driver
|   +-- map.rs          - LED_MAP: world position of every LED (generated)
|   +-- geom.rs         - per-LED distances, angles and grid buckets (generated)
|   +-- grid.rs         - spatial lookup: the LEDs near a point or segment
+-- patterns/
|   +-- mod.rs          - Pattern traits and shared helpers
|   +-- transition.rs   - blends from one pattern to the next
|   +-- ambient/        - patterns that ignore sound
|   +-- reactive/       - patterns driven by sound
+-- input/
    +-- mod.rs          - InputEvent queue
    +-- buttons.rs      - d-pad and switch polling, debounce
    +-- buttons/        - gpio.rs (wired direct) or expander.rs (input board over I2C)
    +-- ir.rs, nec_capture.rs, nec_rx.rs - IR remote, NEC decoded on a BIO core
    +-- previewer.rs    - on-screen controls from the previewer
```

Code shared with the ear (frame format, sound tuning) lives in [`../shared/`](../shared/).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). Optional features can be added under **Extra Features** in the extension settings:

| Feature | Purpose |
|---|---|
| `previewer` | Send LED frames over USB serial to the browser simulator instead of driving WS2812, and take on-screen control input |
| `bringup` | Stream boot stages, a heartbeat, audio link stats and IR activity over USB serial. Silent when combined with `previewer`, whose serial stream is binary |
| `input-board` | Read the d-pad and switch from the separate input board (MCP23008 over I2C) instead of eye GPIOs |

## Using the previewer

The previewer is a browser-based LED simulator at `../../triangel previewer/`. It can receive live frames from this firmware over USB serial.

1. Enable the `previewer` feature in the Baochip extension settings and build
2. Flash and boot the DABAO
3. In the previewer directory: `npm install` (first time), then `node bridge.js`
4. Open `index.html` in a browser - it will show the live pattern output

Bridge defaults: COM3, 921600 baud, WebSocket port 8080. Override via `bridge.config.json` or CLI flags.

## Adding a pattern

See [PATTERNS.md](PATTERNS.md).

## Sound reactivity

Each render frame, reactive patterns get an `Audio` snapshot built from the ear's frames: 24 band levels, a per-band onset (`rise`), overall loudness relative to recent music, and one-frame `beat` and `drop` flags. Detection thresholds and every pattern's sound tuning are in [`../shared/src/tuning.rs`](../shared/src/tuning.rs).

Sound mode is set by the 3-position switch, or cycled by the remote's gear button:

| Position | Behaviour |
|---|---|
| Off | Always the ambient setlist |
| Auto | The reactive setlist once the room has been loud for a sustained stretch, back to ambient once it has been quiet for one |
| On | Always the reactive setlist |
