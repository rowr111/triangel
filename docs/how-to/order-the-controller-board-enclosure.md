# How to order the controller board enclosure

The enclosure holds the controller board on the wall next to the Triangel, where you can reach the buttons and the remote can see it. It has two parts: a back tray that hangs on the wall, and a front plate with openings for the buttons, the sound switch, the remote's receiver and the microphone. Its labels are engraved into it: the triangle tile ports on the top, 12 V ONLY by the jack, and the controls on the front.

Like the brackets, it is 3D printed in nylon by [JLC3DP](https://jlc3dp.com), which also presses in the inserts.

## 1. Get the files

| Part | STEP file | Insert drawing |
|---|---|---|
| Back tray | [`controller board enclosure back.step`](../../hardware/controller-enclosure/controller%20board%20enclosure%20back.step) | [`controller board enclosure back insert drawing.pdf`](../../hardware/controller-enclosure/controller%20board%20enclosure%20back%20insert%20drawing.pdf) |
| Front plate | [`controller board enclosure front.step`](../../hardware/controller-enclosure/controller%20board%20enclosure%20front.step) | none |

## 2. Upload them to JLC3DP

On [jlc3dp.com](https://jlc3dp.com), start a 3D printing quote and upload the two STEP files. For each one, choose:

- **Material:** MJF nylon (PA12)
- **Color:** black
- **Quantity:** 1

## 3. Add the inserts

Turn on JLC3DP's threaded-insert service for the back tray:

- **Insert:** M3x4x3
- **How many:** 4, in the bosses inside the tray
- **Drawing:** attach the back tray's insert drawing

The front plate has no inserts. Four M3x8 screws go through it and the controller board into the back tray's inserts, holding the plate and the board together.
