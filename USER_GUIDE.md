# Triangel - user guide

## Updating the firmware

Two small boards, called DABAO boards, sit in sockets on the controller board and
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
