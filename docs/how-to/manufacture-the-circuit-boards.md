# How to manufacture the circuit boards

A Triangel has two kinds of circuit board, all ordered from [JLCPCB](https://jlcpcb.com) with the parts fitted. The files JLCPCB needs are ready in each board's `production/` folder.

FYI there is a minimum of 5 boards when ordering from JLCPCB, so you must order 5 when ordering the controller board.  They also only offer orders in increments of 5 - you need 25 triangle tiles, but I suggest to order 30 in case some do not work/break later.  

YOU MUST ALSO DO ASSEMBLY!

Special notes for ordering:

### PCB
 - black color
 - Surface Finish: ENIG
 - for the controller, Specify Stackup: Yes, then choose JLC04161H-7628 (the USB lines are sized for it)
 - for the controller, Via Covering: Epoxy Filled & Capped


### PCBA
 - all boards: both sides
 - for the triangle tiles, you must bake the LED component
 - for the controller, specify NO CLEANING (cleaning can break the microphone; the board also has a note silkscreened)
 - for the controller, add the two chips yourself; see below
 - for the controller, please ship with ESD + Cardboard


| Board | How many | Folder |
|---|---|---|
| Triangle tile | 25 | [`hardware/triangle`](../../hardware/triangle) |
| Controller board (v3) | 1 | [`hardware/controller`](../../hardware/controller) |

The controller board's two chips, the eye and the ear (U5 and U7), are Baochip-1x chips, part number BAO1X2S4F-WA. They aren't in JLCPCB's parts library yet, so you need two for each controller board you have assembled. Either get them in the assembly order through JLCPCB's [Global Sourcing](https://jlcpcb.com/help/article/How-to-use-JLCPCB-Global-Sourcing-Parts-Service), which buys them from [DigiKey](https://www.digikey.com/en/products/detail/baochip/BAO1X2S4F-WA/30371724), or buy them yourself and send them to JLCPCB.

J1 and J2, two 3-pin serial headers on the controller board, are left off on purpose. They are only for debugging.

Each board's `production/` folder holds these files:

- the board's zip file (`triangle.zip` or `controller.zip`) (this is what you upload for JLCPCB where it says 'Add gerber file' at the top)
- `bom.csv`, the parts list (for assembly)
- `positions.csv`, where each part goes (for assembly)