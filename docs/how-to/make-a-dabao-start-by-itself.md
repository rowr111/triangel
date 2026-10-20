# How to make a dabao start by itself

A new dabao by default waits in bootloader mode (drive mode) every time it starts up, until someone presses PROG. In a triangel that means the patterns do not start at power-up. Set each board, once, to start its firmware by itself.

## 1. Put it in bootloader mode

Plug the dabao into your computer. A new dabao starts in bootloader mode, so a drive appears. If it does not, hold **PROG** while you press **RESET**.

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

## 2. Open its serial port

Open its serial port with a terminal program: PuTTY on Windows, or `screen` on Linux and macOS. The speed setting does not matter.

In VS Code with the Baochip extension instead: with the dabao in bootloader mode, click **Set bootloader mode serial port** in the Baochip sidebar and pick the dabao's port. Make sure **Default monitor** says **Bootloader** (click it to change it), then click **Monitor**.

## 3. Turn off the wait

Type `bootwait disable` and press Enter.

If it answers `Command not recognized`, type the command again.

From then on it starts its firmware right away at power-up. `bootwait enable` undoes it.
