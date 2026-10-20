# Triangel - user guide

## Controls

The buttons on Triangel's controller board and the remote do the same things.

| Control | What it does |
|---|---|
| Up / Down | Brightness, in 7 steps. Holding the button on the remote keeps stepping. |
| Left / Right | Previous or next pattern, with a 3 second transition. Extra presses are remembered and played in turn. |
| Center | Hold: stays on the current pattern until you press it again. Nothing on the Triangel shows that hold is on. |
| Switch | Sound patterns Off, Auto or On. |
| Gear (remote) | Steps through Off, Auto and On, like the switch. |
| TV (remote) | Nothing. |

Left alone, Triangel moves to the next pattern by itself: every 3 minutes for the ambient patterns and every 5 minutes for the sound patterns.

In Auto, Triangel changes to the sound patterns after about 30 seconds of music, and back after about 30 seconds of quiet.

Nothing is remembered when the power goes off. Triangel starts at middle brightness with hold off, and takes its sound setting from the switch, whatever the gear button last chose.

## Updating the firmware

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

The eye and ear boards sit in sockets on the controller board labeled EYE and EAR, each with its own USB-C connector. To load new firmware onto them, follow [How to update your triangel's firmware](docs/how-to/update-your-triangel.md). If you build the firmware yourself, see [How to build and flash the boards](docs/how-to/build-and-flash.md) instead.
