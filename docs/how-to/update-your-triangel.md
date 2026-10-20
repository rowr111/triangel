# How to update your triangel's firmware

No programming tools needed: you copy files onto each dabao board.

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

## 1. Download the firmware

Download the zip from the [latest release](https://github.com/rowr111/triangel/releases/latest) and unzip it. It has two folders, `eye` and `ear`, one for each board.

## 2. Put the eye in its bootloader

The eye and ear boards sit in sockets on the controller board labeled EYE and EAR.

Before you plug anything in, unplug the triangle tiles from the controller board, unless the 12 V supply is plugged in and stays plugged in.

Hold **PROG** on the eye while you plug its USB cable into your computer. If the board is already powered by the controller board, hold **PROG** while you press **RESET** instead.

A new drive appears on your computer.

> [!NOTE]
> A brand-new dabao, such as a replacement, behaves differently: it starts in its bootloader every time it gets power, so the drive appears without holding PROG. That also means a triangel with a new dabao will not start its patterns at power-up until someone presses PROG. To make it start by itself, see [How to make a dabao start by itself](make-a-dabao-start-by-itself.md).

## 3. Copy the files

Copy everything in the `eye` folder onto the drive.

## 4. Start the new firmware

Press **PROG**. The drive goes away and the eye starts the new firmware.

## 5. Do the same for the ear

Repeat steps 2 to 4 on the ear, with the files from the `ear` folder.
