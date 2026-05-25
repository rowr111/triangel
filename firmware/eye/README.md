# triangel - eye firmware

LED controller firmware for the triangel fixture. Runs on a Baochip-1x under [Xous OS](https://betrusted.io/xous-book/) (DABAO dev board used during development). See [`../README.md`](../README.md) for the overall system and physical fixture description.

## What this is

The **eye** chip drives 600 WS2812 LEDs across 25 triangle PCBs. It manages:

- A 30 fps render loop driving the LED chain via a BIO co-processor core
- Two pattern setlists (ambient and sound-reactive), cycling every ~3 minutes
- D-pad + 3-position switch controls for brightness, pattern stepping, hold, and sound mode
- IR remote (same functions as the physical controls)
- Receives mel frequency data from the **ear** chip over UART - 24 band values for pattern reactivity plus a sustained-activity flag for Auto sound mode switching

## Hardware

| Thing | Detail |
|---|---|
| Chip | Baochip-1x - 350 MHz VexRiscv RV32-IMAC, 2 MB SRAM, 4 MB ReRAM |
| BIO cores | 4x PicoRV at 700 MHz - one used for WS2812 bit-timing |
| LED output | Single WS2812 daisy chain, 600 LEDs, BIO pin 5 |
| LED variant | WS2812B (`LedVariant::B`) |
| Button board | D-pad (5 buttons) + 3-position switch + IR receiver |

## Project structure

```
src/
+-- main.rs          - entry point; 30 fps render loop
+-- led/
|   +-- mod.rs       - LedOutput: wraps WS2812 BIO driver or USB serial previewer
|   +-- map.rs       - LED_MAP: 600-entry const array of world positions (generated)
+-- patterns/        - Pattern trait, Envelope, utilities, and individual pattern files
+-- setlist.rs       - ambient and reactive pattern lists, cycling timer, brightness, sound mode state
+-- input/           - InputEvent queue, d-pad/switch and IR remote (stubs)
+-- audio.rs         - mel data receiver from ear chip (stub - UART TBD)
```

The bringup shell (hardware debugging REPL) lives in `src/cmds/`, `src/shell.rs`, `src/repl.rs` and is compiled only with `--features bringup`.

Wire protocol types shared between eye and ear live in [`../shared/`](../shared/) (`triangel-shared` crate).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). Optional features can be added under **Extra Features** in the extension settings:

| Feature | Purpose |
|---|---|
| `previewer` | Send LED frames over USB serial to the browser simulator instead of driving WS2812 |
| `bringup` | Enable the hardware debug REPL over USB serial |

These two are mutually exclusive - both claim the USB serial port and will conflict if combined.

## Using the previewer

The previewer is a browser-based LED simulator at `../../triangel previewer/`. It can receive live frames from this firmware over USB serial.

1. Enable the `previewer` feature in the Baochip extension settings and build
2. Flash and boot the DABAO
3. In the previewer directory: `npm install` (first time), then `node bridge.js`
4. Open `index.html` in a browser - it will show the live pattern output

Bridge defaults: COM3, 921600 baud, WebSocket port 8080. Override via `bridge.config.json` or CLI flags.

## Adding a pattern

See [PATTERNS.md](PATTERNS.md) for a full guide including world coordinates, available utilities, and how to write sound-reactive patterns with custom envelopes.

## Sound reactivity

The **ear** chip sends 24 mel-frequency band values per frame over UART at ~30 fps. The eye chip applies a fixed attack/decay envelope (attack 0.25, decay 0.02) and exposes two things to patterns: `sound_level` (a single smoothed scalar 0.0-1.0) and `current_mel()` (raw per-band values for patterns that want to apply their own smoothing).

Sound mode is controlled by the 3-position switch:

| Position | Behaviour |
|---|---|
| Off | Always use ambient setlist |
| Auto | Switch to reactive setlist when the ear chip reports sustained activity |
| On | Always use sound-reactive setlist |
