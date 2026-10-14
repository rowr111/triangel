# Triangel - user guide

## Controls

The buttons on the fixture and the remote do the same things.

| Control | What it does |
|---|---|
| Up / Down | Brightness, in 7 steps. Holding the button on the remote keeps stepping. |
| Left / Right | Previous or next pattern, with a 3 second transition. Extra presses are remembered and played in turn. |
| Center | Hold: stays on the current pattern until you press it again. Nothing on the fixture shows that hold is on. |
| Switch | Sound patterns Off, Auto or On. |
| Gear (remote) | Steps through Off, Auto and On, like the switch. |
| TV (remote) | Nothing. |

Left alone, the fixture moves to the next pattern by itself: every 3 minutes for the ambient patterns and every 5 minutes for the sound patterns.

In Auto, the fixture changes to the sound patterns after about 30 seconds of music, and back after about 30 seconds of quiet.

Nothing is remembered when the power goes off. The fixture starts at middle brightness with hold off, and takes its sound setting from the switch, whatever the gear button last chose.

## Updating the firmware

Two small boards, called dabao boards, sit in sockets on the controller board and
each runs its own firmware. The sockets are labeled EYE and EAR. Each board has a
RESET and a PROG button, and its own USB-C connector.

Both boards start their firmware as soon as they get power - there is nothing to
press.

To load new firmware, put a board into its bootloader:

1. Plug a USB-C cable from your computer into the board you want to update.
2. Hold down PROG.
3. Still holding PROG, press RESET and release it.
4. Keep holding PROG for about another second, then release it.

The board appears on your computer as a removable drive. Copy the new `.uf2` files
onto it, then press PROG once more to restart into the new firmware.

RESET on its own will not get you into the bootloader - it just restarts the
firmware. PROG has to be held as the board comes out of reset.
