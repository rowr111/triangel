# How to build a Triangel

Everything you need for a finished Triangel, and how to put it together. See the inventory list below as well as manufacturing instructions.

You can build it with or without the frame. If you decide you want a frame, you'll use a different three-way bracket (in addition to the frame itself).

## What you need

### Circuit boards

See [How to manufacture the circuit boards](manufacture-the-circuit-boards.md).

- 25 triangle tiles
- 1 controller board (v2)
- 1 input board

### Printed parts

| Part | How many | Page |
|---|---|---|
| Six-way brackets | 6 | [How to order the six-way and three-way brackets](order-the-brackets.md) |
| Three-way brackets (different depending on frame/no frame)  | 12 | [How to order the six-way and three-way brackets](order-the-brackets.md) |
| Wall brackets | 3 | [How to order the wall brackets](order-the-wall-brackets.md) |
| Input board enclosure, back tray and front plate | 1 of each | [How to order the enclosures](order-the-enclosures.md) |
| Controller board bracket | 1 | [How to order the enclosures](order-the-enclosures.md) |
| Frame (optional): straight pieces | 12 | [How to order the frame](order-the-frame.md) |
| Frame (optional): corner pieces | 3 | [How to order the frame](order-the-frame.md) |

### Equipment needed

- soldering iron (and its related items)
- screwdriver that fits the screw heads you choose

### Things to buy

- 2 [dabao](https://www.crowdsupply.com/baochip/dabao) boards, to be used as the eye and the ear
- headers for the dabao boards
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
  - 1 from the controller board to the input board
    - **IMPORTANT** MUST be USB-C 4 with video capabilities!  
    - example: https://www.amazon.co.jp/dp/B0C93G2M83
- screws (see below)

### Screws

All the screws need to be BUTTON head, but you can get philips/hex type, it's up to you.  I used phillips.

| Screw | How many | Where |
|---|---|---|
| M3x6 (brass) | 75 | Connect triangle tiles to their brackets and wall brackets |
| M3x8 (black) | 4 | Connect controller board into its bracket |
| M3x8 (black) | 12 | FRAME ONLY: through each three-way bracket's tab into the frame |
| M3x6 (black) | 4 | Connect front plate of input board into the back |
| normal wall screws (up to M6 or #12) | 7 | For the wall brackets, the controller board bracket and the input board enclosure |

### Optional

- Numbered stickers for the triangle tiles, and overlays for the input board and the remote; see [graphics](../../graphics/README.md).

## 1. Set up the dabao boards

0. Solder headers onto each dabao
1. Flash the eye and the ear, as in [How to update your Triangel's firmware](update-your-triangel.md). Do this with each dabao on its own, before it goes into the controller board.
2. Make each one start by itself, as in [How to make a dabao start by itself](make-a-dabao-start-by-itself.md).
3. Plug them into the controller board's sockets labeled EYE and EAR.

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
- the third USB-C connector to the input board

## 5. Add the frame (with the frame only)

Join the 15 pieces around the Triangel. The triangle on the end of each piece drops into the next piece's socket from the front. Each straight piece's slot goes over a bracket's tab; screw an M3x8 in from the back, through the tab, into the insert in the slot. The corner pieces are held in place by the joints on either side.

## 6. Put on the wall brackets

Screw a wall bracket to each of the three corners of the greater 25-triangel tile piece, with an M3 screw through the front of the corner triangle tile into the wall bracket.

## 7. Mount it

Note - you can do these in the order that works for you - I suggest trying it out and marking the locations with pencil first before attaching anything to walls.

1. Screw the controller board to its bracket with four M3x8 screws, and screw the bracket to the wall where it will sit behind the Triangel.
2. Screw the wall brackets to the wall.
3. Hang the input board's back tray on the wall where you can reach it: on two screws through its keyholes, plus one locking screw. Then screw the front plate and the input board onto it.

## 8. Plug in the power

Connect the power supply to the controller board's 12 V jack, then to the mains. The patterns start by themselves.

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

## 9. Check it

- All 25 triangle tiles light.
- The buttons, the sound switch and the remote work; see the [user guide](../../USER_GUIDE.md#controls).
- With the sound switch on **On**, the sound patterns follow music played from real speakers.

If something doesn't work, see [Diagnostic output](../reference/diagnostic-output.md) and [How to check a microphone that is not working](check-the-microphone.md).
