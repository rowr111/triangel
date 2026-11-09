# How to order the frame

The frame is an optional black border that the whole Triangel sits inside. It is made of 15 printed pieces that join with triangle-shaped joints. Like the brackets, it is 3D printed in nylon by [JLC3DP](https://jlc3dp.com).

You need:

| Piece | How many | Inserts in each |
|---|---|---|
| Straight piece | 12 | 1 |
| Corner piece | 3 | none |

A Triangel with the frame also needs the three-way brackets with a tab, instead of the plain ones; see [How to order the six-way and three-way brackets](order-the-brackets.md).

## 1. Get the files

| Piece | STEP file | Insert drawing |
|---|---|---|
| Straight piece | [`frame_straight.step`](../../hardware/frame/frame_straight.step) | [`FRAME_STRAIGHT_insert_drawing.pdf`](../../hardware/frame/FRAME_STRAIGHT_insert_drawing.pdf) |
| Corner piece | [`frame_corner.step`](../../hardware/frame/frame_corner.step) | none |

All three corners are the same piece.

## 2. Upload them to JLC3DP

On [jlc3dp.com](https://jlc3dp.com), start a 3D printing quote and upload both STEP files.

For each one, choose:

- **Material:** MJF nylon (PA12)
- **Color:** black
- **Quantity:** 12 straight pieces and 3 corner pieces

I suggest ordering at least one extra of each, in case anything goes wrong and to have a spare on hand.

## 3. Add the inserts

Turn on JLC3DP's threaded-insert service for the straight piece only:

- **Insert:** M3x4x3
- **How many:** 1 in each straight piece
- **Drawing:** attach `FRAME_STRAIGHT_insert_drawing.pdf`
