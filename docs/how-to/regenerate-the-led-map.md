# How to regenerate the LED map

The LED map is where every one of the 600 LEDs sits, in millimeters, and its place in the wiring. Patterns read it through `LED_MAP`, and the previewer uses it to draw the triangel.

You only need to regenerate it if the triangle tiles change shape or the gap between them changes. The map is generated, so do not edit it by hand.

## 1. Change the layout

The layout is set at the top of `firmware/eye/tools/gen_map.py`:

- `LOCAL_LEDS`: where the 24 LEDs sit on one triangle tile, from the KiCad board file
- `LOCAL_CX`, `LOCAL_CY`: the center of one triangle tile, in the same coordinates
- `BOARD_GAP`: the gap between neighboring triangle tiles, in mm

The triangle tiles are numbered in wiring order, as in [Triangle tiles and wiring](../reference/triangle-tiles-and-wiring.md). Wiring them differently would mean changing `build_boards` in the same script, and the chain split written out in `MAP_RS_HEAD`.

## 2. Regenerate the map

From `firmware/eye`:

```
python tools/gen_map.py
```

It writes `map.rs` and `led_map.js`.

## 3. Regenerate the values worked out from it

Still from `firmware/eye`:

```
python tools/gen_geom.py
```

It writes `geom.rs`.

## 4. Check it

Build the eye with the `previewer` option, flash it and open the previewer, as in [How to run the previewer](run-the-previewer.md). The triangle tiles should be where you expect, and the patterns should move across them smoothly.

To check that the generated files match the script without writing them, run either script with `--check`.
