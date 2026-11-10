# How to build a Triangel

Everything you need for a finished Triangel, and how to put it together. See the inventory list below as well as manufacturing instructions.

You can build it with or without the frame. If you decide you want a frame, you'll use a different three-way bracket (in addition to the frame itself).

## What you need

### Circuit boards

See [How to manufacture the circuit boards](manufacture-the-circuit-boards.md).

- 25 triangle tiles
- 1 controller board (v3)

### Printed parts

| Part | How many | Page |
|---|---|---|
| Six-way brackets | 6 | [How to order the six-way and three-way brackets](order-the-brackets.md) |
| Three-way brackets (different depending on frame/no frame)  | 12 | [How to order the six-way and three-way brackets](order-the-brackets.md) |
| Wall brackets | 3 | [How to order the wall brackets](order-the-wall-brackets.md) |
| Controller board enclosure, back tray and front plate | 1 of each | [How to order the controller board enclosure](order-the-controller-board-enclosure.md) |
| Frame (optional): straight pieces | 12 | [How to order the frame](order-the-frame.md) |
| Frame (optional): corner pieces | 3 | [How to order the frame](order-the-frame.md) |

### Equipment needed

- screwdriver that fits the screw heads you choose

### Things to buy

- An IR remote
- A 12V power supply (12V 5A 60W will work), and a cable from it to the controller board's 12 V jack
  - example: https://www.amazon.co.jp/dp/B0CDL7PR7R
- USB-C to USB-C cables:
  - 23 to connect the triangle tiles
    - right angle connector, 15cm
    - example: https://www.amazon.co.jp/dp/B0DSTW113Z
  - 2 from the controller board to the triangle tiles
    - 1m or 2m cables are recommended, it depends on where you choose to place the controller.
    - example: https://www.amazon.co.jp/dp/B0GY4BWWMM
- screws (see below)

### Screws

All the screws need to be BUTTON head, but you can get philips/hex type, it's up to you.  I used phillips.

| Screw | How many | Where |
|---|---|---|
| M3x6 (brass) | 75 | Connect triangle tiles to their brackets and wall brackets |
| M3x8 (black) | 4 | Connect front plate of controller board enclosure, through the controller board, into the back |
| M3x8 (black) | 12 | FRAME ONLY: through each three-way bracket's tab into the frame |
| normal wall screws (up to M6 or #12) | 3 | For the wall brackets |
| wall screws, M4 or #8, with a head under 8.5 mm across | 3 | For the controller board enclosure: two through its keyholes and one locking screw |

### Optional

- Numbered stickers for the triangle tiles, and an overlay for the remote; see [graphics](../../graphics/README.md).

## 1. Flash the controller board

Flash the eye and the ear, the controller board's two chips, before the board goes into its enclosure. Each has its own USB-C port, labeled EYE or EAR, on the back of the board.

> TBD: once a v3 controller board has been built: the steps to flash both the ear and the eye on the board

## 2. Lay out the triangle tiles

Lay out the 25 triangle tiles, each turned so its OUT connector is toward its point. See [Triangle tiles and wiring](../reference/triangle-tiles-and-wiring.md) for the layout and a photo of the back.

If you like, mark each triangle tile's number with the stickers first.

## 3. Screw on the brackets

Screw each triangle tile to the brackets behind it, with an M3 screw from the front of the triangle tile into each insert:

- a six-way bracket at each point inside the Triangel where six triangle tiles meet
- a three-way bracket along each outer edge, where three meet

Leave the big corner holes for the wall brackets. The brackets' ridges sit in the gaps between triangle tiles and line them up.

There is no particular order for you need to screw them together, as long as they are aligned as in the layout photo above.

### With the frame

Use the three-way brackets with a tab, each tab pointing outward.

## 4. Connect the cables

Connect the triangle tiles in two chains, each from OUT to the next triangle tile's IN:

- triangle tiles 1 to 12
- triangle tiles 13 to 25

There is no cable between 12 and 13. Then, from the controller board:

- **OUT TO 1-12** to triangle tile 1's IN
- **OUT TO 13-25** to triangle tile 13's IN

## 5. Add the frame (with the frame only)

Join the 15 pieces around the Triangel. The triangle on the end of each piece drops into the next piece's socket from the front. Each straight piece's slot goes over a bracket's tab; screw an M3x8 in from the back, through the tab, into the insert in the slot. The corner pieces are held in place by the joints on either side.

## 6. Put on the wall brackets

Screw a wall bracket to each of the three corners of the greater 25-triangel tile piece, with an M3 screw through the front of the corner triangle tile into the wall bracket.

## 7. Mount it

Note - you can do these in the order that works for you - I suggest trying it out and marking the locations with pencil first before attaching anything to walls.

1. Hang the controller board enclosure's back tray on the wall next to the Triangel, where you can reach the buttons and the remote can see the front: on two screws through its keyholes, plus one locking screw. Then put the controller board in, plug in its cables, and screw the front plate and the board onto the tray with four M3x8 screws. Don't cover the front plate: the microphone listens through a small hole in it.
2. Screw the wall brackets to the wall.

## 8. Plug in the power

Connect the power supply to the controller board's 12 V jack, at the bottom of the enclosure, then to the mains. The patterns start by themselves.

Only use a 12 V power supply. Any other voltage can damage the controller board and the triangle tiles.

## 9. Check it

- All 25 triangle tiles light.
- The buttons, the sound switch and the remote work; see the [user guide](../../USER_GUIDE.md#controls).
- With the sound switch on **On**, the sound patterns follow music played from real speakers.

If something doesn't work, see [Diagnostic output](../reference/diagnostic-output.md) and [How to check a microphone that is not working](check-the-microphone.md).
