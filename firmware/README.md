# Triangel - controller firmware

The Triangel fixture is driven by two Baochip-1x chips running [Xous OS](https://betrusted.io/xous-book/) (dabao dev boards used during development):

| Chip | Role |
|---|---|
| **eye** | Drives 600 WS2812 LEDs, handles controls, renders patterns |
| **ear** | Captures audio, runs a 24-band filterbank, streams the result to eye over UART |

See [How Triangel works](../docs/explanation/how-triangel-works.md) and [How sound becomes light](../docs/explanation/how-sound-becomes-light.md).

## Physical fixture

See [Triangle tiles and wiring](../docs/reference/triangle-tiles-and-wiring.md).

## Eye <-> ear communication

The ear sends frames to the eye over a single UART wire. The frame carries 24 band levels, the overall loudness (absolute and relative to recent music), how sharply the spectrum just rose, and the bass level. The eye turns those into beats, drops, per-band onsets and the Auto sound-mode decision.

The baud rate and frame layout are defined once, in the [`shared/`](shared/) crate that both chips build against; see [The ear-to-eye link](../docs/reference/ear-to-eye-link.md) for the byte layout.

## Building

To set up a computer, see [How to set up a development machine](../docs/how-to/set-up-a-development-machine.md). Before sending changes, install the commit hook described in [CONTRIBUTING.md](../CONTRIBUTING.md).

## Regenerating the LED map

See [How to regenerate the LED map](../docs/how-to/regenerate-the-led-map.md).
