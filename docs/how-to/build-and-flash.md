# How to build the firmware and flash the boards

The eye and the ear are built and flashed the same way. Both use the kernel files in `firmware/xous_build/`; to get them, see [Get the kernel files](set-up-a-development-machine.md#4-get-the-kernel-files).

## 1. Build

### Choose build options

The eye can be built with these options:

- `previewer`: show the LEDs in the browser previewer
- `combined-board`: for the older all-in-one controller board
- `bringup`: print boot progress and diagnostics over USB serial

The ear has no options.

### With VS Code and the Baochip VSCode Extension

Open `firmware/eye`. With the extension set up as in [Set up a way to build](set-up-a-development-machine.md#5-set-up-a-way-to-build), put your build options in the workspace settings:

```json
"baochip.outOfTree.extraFeatures": ["previewer"]
```

You can also set them under **Extra features** from the gear icon in the [Baochip sidebar](../images/baochip-sidebar.png); the gear appears when the mouse is over the **Baochip** heading.

Then click **Build (cargo build)** in the sidebar.

### From the command line

From `firmware/eye`, build, then turn the build into `apps.uf2`:

```
cargo build --release --features previewer
xous-app-uf2 --elf target/riscv32imac-unknown-xous-elf/release/eye
```

For the ear, run the same from `firmware/ear`, with `ear` at the end of the path.

## 2. Put the board in bootloader mode

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

- **A new dabao:** press RESET. It starts in bootloader mode.
- **A dabao set to start its firmware at power-up** (see [Start without pressing PROG](#start-without-pressing-prog)): hold PROG while you press RESET, or while you plug it in. On these boards, RESET alone just restarts the firmware.

Either way, the board appears on your computer as a drive.

## 3. Flash

### With VS Code

Click **Flash device** in the Baochip sidebar. Or, to build, flash and start the firmware in one go, click **Build - Flash - Boot** instead of building first.

### From the command line

Copy these onto the drive:

- `firmware/xous_build/loader.uf2`
- `firmware/xous_build/xous.uf2`
- the `apps.uf2` you built

## 4. Start the firmware

Press PROG.

When the kernel files change, flash both boards, kernel and firmware together.

## Start without pressing PROG

A new dabao waits in bootloader mode at every power-up until you press PROG. To have it start its firmware straight away, see [How to make a dabao start by itself](make-a-dabao-start-by-itself.md). Do this once on each board.
