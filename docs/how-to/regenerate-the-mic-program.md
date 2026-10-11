# How to regenerate the mic program for different pins

The microphone is clocked by a small precompiled BIO program, `firmware/ear/src/i2s_bio.rs`. It is generated from `sw/i2s/main.c` in [bio-sim](https://github.com/rowr111/bio-sim), on the `is2-mic-troubleshooting` branch. The microphone's pins are compiled into it, so changing the wiring means regenerating it; a firmware constant will not do it.

The pins today, which match the schematic and `firmware/ear/src/pins.rs`: BCLK = PB1, SD = PB2, WS = PB3.

1. Once, install the compiler the build uses:

   ```
   pip install ziglang
   ```

2. In `bio-sim/sw/i2s/main.c`, set the pin defines. The number is the PBx pin:

   ```c
   #define WS_PIN  3
   #define SCK_PIN 1
   #define SD_PIN  2
   ```

3. From `bio-sim/sw`, run the build, which rewrites `bio-sim/sw/i2s/i2s.rs`. Where Python is called `python3`, use that:

   ```
   python -m ziglang build -Dmodule=i2s -Demit-listing=false
   ```

4. Copy `bio-sim/sw/i2s/i2s.rs` over `firmware/ear/src/i2s_bio.rs`.
5. Set the same three numbers in `firmware/ear/src/pins.rs` and in the check below the rate constants in `firmware/ear/src/audio.rs`.
6. Update the bio-sim commit named in the comment above `mod i2s_bio;` in `firmware/ear/src/main.rs`.
7. Rebuild the ear's firmware.
