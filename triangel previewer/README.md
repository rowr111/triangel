# Triangel Previewer

Browser-based simulator for the Triangel LED installation — 600 LEDs across 25 triangle PCBs. Open `index.html` directly in a browser, no server needed.

## Requirements

- **Node.js** — required only for `bridge.js` (live Baochip data) and `generate_map.js` (regenerating the LED map). 
- **npm packages** — install once before using the bridge: `npm install`

## Files

| File | Purpose |
|---|---|
| `index.html` | Renderer — open this in a browser |
| `animation.js` | **Edit this** to write animations |
| `led_map.js` | Pre-generated LED positions — do not edit by hand |
| `generate_map.js` | Regenerates `led_map.js` — run if geometry changes |
| `bridge.js` | Serial-to-WebSocket bridge for live Baochip data |
| `bridge.config.json` | Bridge configuration |

## Writing animations

Edit `animation.js`. The single function `getFrame(ledMap, t)` is called every frame (~60fps):

- **`ledMap`** — array of 600 objects: `{wx, wy, boardId, localIdx, chainIdx}`
  - `wx`, `wy` — world position in mm; origin top-left, y increases downward
  - World spans roughly 0–517mm (x) × 0–436mm (y), apex at bottom-center
  - `chainIdx` — position in the physical daisy chain (0–599); see chain wiring below
- **`t`** — timestamp in milliseconds (from `requestAnimationFrame`)
- **Returns** — array of 600 `[r, g, b]` values (0–255)

Reload the browser after saving to see changes.

### Sample animations

Switch between them by changing the `return` line at the bottom of `animation.js`:

| Function | Description |
|---|---|
| `staticWhite` | All LEDs dim white. Useful for verifying geometry. |
| `rainbowX` | Hue cycles across the x axis, scrolling over time. |
| `apexRipple` | Orange pulse radiating outward from the bottom apex. |
| `horizontalScan` | Cyan/blue band sweeping top to bottom, frequency-band style. |

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

The Baochip sends **1800 bytes per frame** — one RGB triplet per LED, in chain order. Byte offset for a given LED is `chainIdx * 3`. The bridge forwards each complete frame as a binary WebSocket message.

## Chain wiring

Single daisy chain, 600 LEDs in series. Traversal order:

- **Row by row, top to bottom**
- **Right to left within each row**
- **D1→D24 within each board**

Board order in chain (chain positions 0–599):

| Chain pos | Board IDs (right → left) |
|---|---|
| 0–215 | Row 1: boards 9, 8, 7, 6, 5, 4, 3, 2, 1 |
| 216–383 | Row 2: boards 16, 15, 14, 13, 12, 11, 10 |
| 384–503 | Row 3: boards 21, 20, 19, 18, 17 |
| 504–575 | Row 4: boards 24, 23, 22 |
| 576–599 | Row 5: board 25 |

`chainIdx` in `led_map.js` encodes this for every LED. When the Baochip streams a frame over USB, byte triplet `chainIdx * 3` is the RGB for that LED.

## Regenerating the LED map

Only needed if board geometry or gap size changes:

```powershell
node generate_map.js
```

Tweak `BOARD_GAP` in `generate_map.js` (currently 2mm) to adjust spacing between boards.
