# triangel - eye firmware

LED controller firmware for the triangel. Runs on a Baochip-1x under [Xous OS](https://betrusted.io/xous-book/) (dabao dev board used during development). See [`../README.md`](../README.md) for the overall system and physical triangel description.

## What this is

The **eye** chip drives 600 WS2812 LEDs across 25 triangle tiles. It manages:

- A 30 fps render loop driving two LED chains in parallel, one BIO co-processor core each
- Two pattern setlists (ambient and sound-reactive), stepping every 3 minutes (ambient) or 5 minutes (sound-reactive) with a transition between patterns
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

Code shared with the ear (frame format, sound tuning) lives in [`../shared/`](../shared/).

## Building

Build via the Baochip VSCode extension (`buildMode: out-of-tree`). Optional features can be added under **Extra Features** in the extension settings:

| Feature | Purpose |
|---|---|
| `previewer` | Send LED frames over USB serial to the browser simulator instead of driving WS2812, and take on-screen control input |
| `bringup` | Stream boot stages, a heartbeat, audio link stats and IR activity over USB serial. Silent when combined with `previewer`, whose serial stream is binary |
| `combined-board` | For the older all-in-one controller board: read the d-pad and switch from eye GPIOs. Without it they are read from the separate input board (MCP23008 over I2C) |

## Using the previewer

See [How to run the previewer](../../docs/how-to/run-the-previewer.md).

## Adding a pattern

Start with [Your first pattern](../../docs/tutorials/first-pattern.md), then see [How to add a pattern](../../docs/how-to/add-a-pattern.md) and the [Pattern toolkit](../../docs/reference/pattern-toolkit.md).

## Sound reactivity

New to sound patterns? Start with [Your first sound pattern](../../docs/tutorials/first-sound-pattern.md).

Each render frame, reactive patterns get an `Audio` snapshot built from the ear's frames: 24 band levels, a per-band onset (`rise`), overall loudness relative to recent music, and one-frame `beat` and `drop` flags. Detection thresholds and the patterns' sound tuning are in [`../shared/src/tuning.rs`](../shared/src/tuning.rs).

Sound mode is set by the 3-position switch, or cycled by the remote's gear button:

| Position | Behaviour |
|---|---|
| Off | Always the ambient setlist |
| Auto | The reactive setlist once the room has been loud for a sustained stretch, back to ambient once it has been quiet for one |
| On | Always the reactive setlist |
