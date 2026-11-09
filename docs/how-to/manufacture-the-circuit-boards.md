# How to manufacture the circuit boards

A Triangel has three kinds of circuit board, all ordered from [JLCPCB](https://jlcpcb.com) with the parts fitted. The files JLCPCB needs are ready in each board's `production/` folder.

FYI there is a minmum of 5 boards when ordering from JLCPCB, so you must order 5 each when ordering the controller/input boards.  They also only offer orders in increments of 5 - you need 25 triangle tiles, but I suggest to order 30 in case some do not work/break later.  

YOU MUST ALSO DO ASSEMBLY!

Special notes for ordering:

### PCB
 - black color
 - Surface Finish: ENIG


### PCBA
 - all boards: both sides
 - for the triangle tiles, you must bake the LED component
 - for the controller, specify NO CLEANING
 - for the controller, please ship with ESD + Cardboard


| Board | How many | Folder |
|---|---|---|
| Triangle tile | 25 | [`hardware/triangle`](../../hardware/triangle) |
| Controller board (v2) | 1 | [`hardware/controller`](../../hardware/controller) |
| Input board | 1 | [`hardware/input`](../../hardware/input) |

The controller board has sockets for the two [dabao](https://www.crowdsupply.com/baochip/dabao) boards, the eye and the ear, which you buy separately and plug in.

Each board's `production/` folder holds these files:

- the board's zip file (`triangle.zip`, `controller.zip` or `input.zip`) (this is what you update for JLCPCB where it says 'Add gerber file' at the top)
- `bom.csv`, the parts list (for assembly)
- `positions.csv`, where each part goes (for assembly)