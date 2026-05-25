# Triangel Previewer

Browser-based simulator for the Triangel LED installation - 600 LEDs across 25 triangle PCBs. Open `index.html` directly in a browser, no server needed.

## Requirements

- **Node.js** - required only for `bridge.js` (live Baochip data) and `generate_map.js` (regenerating the LED map).
- **npm packages** - install once before using the bridge: `npm install`

## Files

| File | Purpose |
|---|---|
| `index.html` | Renderer - open this in a browser |
| `led_map.js` | Pre-generated LED positions - do not edit by hand |
| `generate_map.js` | Regenerates `led_map.js` - run if geometry changes |
| `bridge.js` | Serial-to-WebSocket bridge for live Baochip data |
| `bridge.config.json` | Bridge configuration |
| `ear_sim.py` | Desktop audio simulator - streams mic or system audio to the ear board over USB serial |

## Live data from the Baochip

When the Baochip is connected, `bridge.js` reads its serial output and streams frames to the browser over a local WebSocket. The browser automatically switches from `animation.js` to live data when the bridge connects, and falls back when it disconnects. A **"bridge live"** indicator appears in the top-right corner.

### Setup

Install dependencies (one time):

```powershell
npm install
```

Edit `bridge.config.json` to set your serial port:

```json
{
  "serial": "COM3",
  "baud": 921600,
  "wsPort": 8080
}
```

Then run the bridge:

```powershell
node bridge.js
```

All config values can be overridden on the command line:

```powershell
node bridge.js --serial COM5 --baud 460800 --ws-port 9000
```

### Frame format

The Baochip sends **1800 bytes per frame** - one RGB triplet per LED, in chain order. Byte offset for a given LED is `chainIdx * 3`. The bridge forwards each complete frame as a binary WebSocket message.

## Chain wiring

Single daisy chain, 600 LEDs in series. Traversal order:

- **Row by row, top to bottom**
- **Right to left within each row**
- **D1->D24 within each board**

Board order in chain (chain positions 0-599):

| Chain pos | Board IDs (right -> left) |
|---|---|
| 0-215 | Row 1: boards 9, 8, 7, 6, 5, 4, 3, 2, 1 |
| 216-383 | Row 2: boards 16, 15, 14, 13, 12, 11, 10 |
| 384-503 | Row 3: boards 21, 20, 19, 18, 17 |
| 504-575 | Row 4: boards 24, 23, 22 |
| 576-599 | Row 5: board 25 |

`chainIdx` in `led_map.js` encodes this for every LED. When the Baochip streams a frame over USB, byte triplet `chainIdx * 3` is the RGB for that LED.

## Desktop audio simulator (ear_sim.py)

`ear_sim.py` streams desktop audio or microphone input to the ear board over USB serial, using the same 1025-byte frame format the real ICS43434 mic produces. This lets the full mel filterbank and UART output path run on real music without the mic circuit being assembled. Build the ear firmware with the `uart-audio` feature to use it.

### Dependencies

```powershell
pip install pyaudiowpatch numpy pyserial
```

### Usage

```powershell
# Stream system speakers (default) to the ear board on COM6
python ear_sim.py

# Use microphone instead
python ear_sim.py --mic

# List available audio devices to find the right index
python ear_sim.py --list-devices

# Specific device index (--mic mode only)
python ear_sim.py --mic --device 2

# Different serial port
python ear_sim.py --port COM4
```

The script auto-detects the WASAPI loopback device for the default speakers. A live bar display shows RMS level and frame rate while running. Press Ctrl+C to stop.

## Regenerating the LED map

Only needed if board geometry or gap size changes:

```powershell
node generate_map.js
```

Tweak `BOARD_GAP` in `generate_map.js` (currently 2mm) to adjust spacing between boards.
