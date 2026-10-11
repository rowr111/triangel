# Triangel Previewer

Shows the fixture's 600 LEDs in a browser, live from an eye built with `--features previewer`.

## Files

| File | Purpose |
|---|---|
| `index.html` | The page. Open it in a browser; no server needed |
| `bridge.js` | Passes frames from the eye's serial port to the page |
| `bridge.config.json` | The bridge's serial port and baud rate |
| `led_map.js` | LED positions, generated - do not edit by hand |
| `generate_map.js` | Writes `led_map.js` |

## Running it

Needs Node.js. Once, install the packages:

```powershell
npm install
```

Set your serial port in `bridge.config.json`:

```json
{
  "serial": "COM3",
  "baud": 921600
}
```

Run the bridge, then open `index.html`:

```powershell
node bridge.js
```

Either setting can be given on the command line instead: `node bridge.js --serial COM5 --baud 460800`.

The indicator in the page's top-right corner:

| Reads | Means |
|---|---|
| bridge live | frames are arriving |
| no frames | the bridge is reachable but the eye is not sending |
| no bridge | the page cannot reach the bridge |

## Frame format

The eye sends 1804 bytes per frame: a 4-byte marker (`FF FF FF FF`), then one RGB triplet per LED in chain order. Colors are capped at 254, so the marker never appears in the data. The bridge strips the marker and forwards the other 1800 bytes; an LED's bytes start at `chainIdx * 3`.

## Chain wiring

LEDs are numbered 0-599 in wiring order, D1 to D24 within each board. The wiring snakes down the fixture:

| Chain pos | Boards, in wiring order |
|---|---|
| 0-215 | Row 1, left to right: 1 to 9 |
| 216-383 | Row 2, right to left: 16 to 10 |
| 384-503 | Row 3, left to right: 17 to 21 |
| 504-575 | Row 4, right to left: 24 to 22 |
| 576-599 | Row 5: 25 |

The fixture drives this as two chains: 0-287 (through board 14) and 288-599.

## Regenerating the LED map

Only if the board geometry or the gap between boards (`BOARD_GAP` in `generate_map.js`) changes:

```powershell
node generate_map.js
```
