# How to order the enclosures

Two printed parts hold the boards that go with the Triangel:

- The **input board enclosure** holds the input board, with the buttons, the sound switch and the remote's receiver, on the wall where you can reach it. It has two parts: a back tray that mounts to the wall, and a front plate with the openings.
- The **controller board bracket** is a plate that holds the controller board on the wall, behind the Triangel.

Like the brackets, they are 3D printed in nylon by [JLC3DP](https://jlc3dp.com).

## 1. Get the files

| Part | How many | STEP file | Insert drawing |
|---|---|---|---|
| Input board enclosure, back tray | 1 | [`input-board-back-tray.step`](../../hardware/input-enclosure/input-board-back-tray.step) | [`BACKTRAY_insert_drawing.pdf`](../../hardware/input-enclosure/BACKTRAY_insert_drawing.pdf) |
| Input board enclosure, front plate | 1 | [`input-board-front-plate.step`](../../hardware/input-enclosure/input-board-front-plate.step) | none |
| Controller board bracket | 1 | [`controller_bracket.step`](../../hardware/brackets/controller%20bracket/controller_bracket.step) | [`CONTROLLER_BRACKET_insert_drawing.pdf`](../../hardware/brackets/controller%20bracket/CONTROLLER_BRACKET_insert_drawing.pdf) |

## 2. Upload them to JLC3DP

On [jlc3dp.com](https://jlc3dp.com), start a 3D printing quote and upload the three STEP files. For each one, choose:

- **Material:** MJF nylon (PA12)
- **Color:** black
- **Quantity:** 1

## 3. Add the inserts

Turn on JLC3DP's threaded-insert service for the back tray and the controller board bracket:

| Part | Insert | How many | Drawing |
|---|---|---|---|
| Back tray | M3x4x3 | 4, in the bosses inside the tray | `BACKTRAY_insert_drawing.pdf` |
| Controller board bracket | M3x4x3 | 4, in the tops of the standoffs | `CONTROLLER_BRACKET_insert_drawing.pdf` |

The front plate has no inserts. Four screws go down through it and the input board into the back tray's inserts, holding the plate and the board together.

## 4. Graphics

If you like, print the input board overlay, which goes on the front plate; see [graphics](../../graphics/README.md).
