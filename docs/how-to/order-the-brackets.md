# How to order the six-way and three-way brackets

The brackets join the 25 triangle tiles into one big triangle. Each sits behind a point where triangle tiles meet, and an M3 screw from the front of each triangle tile goes into a brass insert in the bracket. They are 3D printed in nylon by [JLC3DP](https://jlc3dp.com), which also presses in the inserts.

You need:

| Bracket | How many | Inserts in each |
|---|---|---|
| Six-way, where six triangle tiles meet | 6 | 6 |
| Three-way, along the outer edge where three meet | 12 | 3 |

The three-way bracket comes in two versions, depending on whether your Triangel has the frame; see [How to build a Triangel](build-a-triangel.md).

## 1. Get the files

Each bracket has a STEP file, which JLC3DP prints from, and an insert drawing, which shows where the inserts go.

| Bracket | STEP file | Insert drawing |
|---|---|---|
| Six-way | [`triangle bracket - 6 way.step`](../../hardware/brackets/triangel%20brackets/triangle%20bracket%20-%206%20way.step) | [`BRACKET_6WAY_insert_drawing.pdf`](../../hardware/brackets/triangel%20brackets/BRACKET_6WAY_insert_drawing.pdf) |

For the three-way bracket, use one of these:

### Without the frame

| Bracket | STEP file | Insert drawing |
|---|---|---|
| Three-way | [`triangle bracket - 3 way.step`](../../hardware/brackets/triangel%20brackets/triangle%20bracket%20-%203%20way.step) | [`BRACKET_3WAY_insert_drawing.pdf`](../../hardware/brackets/triangel%20brackets/BRACKET_3WAY_insert_drawing.pdf) |

### With the frame

The three-way bracket has a tab that fits a slot in the frame, and a screw through the tab holds the frame on.

| Bracket | STEP file | Insert drawing |
|---|---|---|
| Three-way with a tab | [`frame_bracket.step`](../../hardware/frame/frame_bracket/frame_bracket.step) | [`BRACKET_3WAY_TAB_insert_drawing.pdf`](../../hardware/frame/frame_bracket/BRACKET_3WAY_TAB_insert_drawing.pdf) |

The tab has no insert: its screw goes into an insert in the frame piece.

## 2. Upload them to JLC3DP

On [jlc3dp.com](https://jlc3dp.com), start a 3D printing quote and upload the two STEP files.

For each one, choose:

- **Material:** MJF nylon (PA12)
- **Color:** black
- **Quantity:** 6 six-way and 12 three-way

I suggest to order at least one additional of each, to have a spare or in case one is not right for some reason.


## 3. Add the inserts

Turn on JLC3DP's threaded-insert service for both brackets:

- **Insert:** M3x4x3
- **How many:** 6 in each six-way bracket, 3 in each three-way bracket
- **Drawing:** attach the bracket's insert drawing
