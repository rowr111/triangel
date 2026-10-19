# How to build and flash the boards

The eye and the ear are built and flashed the same way. Both use the kernel files in `firmware/xous_build/`; to get them, see [Get the kernel files](set-up-a-development-machine.md#4-get-the-kernel-files).

## 1. Build

### Choose build options

The eye can be built with these options:

- `previewer`: show the LEDs in the browser previewer
- `combined-board`: for the older all-in-one controller board
- `bringup`: print boot progress and diagnostics over USB serial

The ear has no options.

### With VS Code and the Baochip VSCode Extension

Open `firmware/eye` and check its workspace settings. The first three are needed for every build; the last one lists your build options:

```json
"baochip.buildTarget": "dabao",
"baochip.outOfTree.kernelMode": "manual",
"baochip.outOfTree.kernelFilesPath": "../xous_build",
"baochip.outOfTree.extraFeatures": ["previewer"]
```

You can also set these from the gear icon in the Baochip extension's sidebar.

Then run **Baochip: Build**.

### From the command line

From `firmware/eye`, build, then turn the build into `apps.uf2`:

```
cargo build --release --features previewer
xous-app-uf2 --elf target/riscv32imac-unknown-xous-elf/release/eye
```

For the ear, run the same from `firmware/ear`, with `ear` at the end of the path.

## 2. Put the board in its bootloader

- **A new dabao:** press RESET. It starts in its bootloader.
- **A dabao set to start its firmware at power-up** (see [Start without pressing PROG](#start-without-pressing-prog)): hold PROG, press and release RESET, and keep holding PROG for about another second. On these boards, RESET alone just restarts the firmware.

Either way, the board appears on your computer as a drive.

## 3. Flash

### With VS Code

Run **Baochip: Flash**.

### From the command line

Copy these onto the drive, in this order:

1. `firmware/xous_build/loader.uf2`
2. `firmware/xous_build/xous.uf2`
3. the `apps.uf2` you built

## 4. Start the firmware

Press PROG.

When the kernel files change, flash both boards, kernel and firmware together.

## Start without pressing PROG

A new dabao waits in its bootloader at every power-up until you press PROG. To have it start its firmware straight away, open its bootloader serial port with a terminal or the extension's monitor, and type:

    bootwait disable

Do this once on each board. `bootwait enable` undoes it.
