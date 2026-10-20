# How to check a microphone that is not working

If the sound patterns do not react to music, this narrows down whether the microphone hears anything, and if not, why. The ear's console runs the tests and prints a verdict after each one.

> [!CAUTION]
> **Controller boards v1 and v2:** never plug a dabao into USB while the triangle tiles are connected and the 12 V supply is not. The triangle tiles, up to 60 watts of lights, would try to run off the dabao. Instead, either:
> - keep the 12 V supply plugged in,
> - unplug the triangle tiles, or
> - take the dabao out of its socket.

## 1. Open the ear's console

Plug the ear into your computer and open the serial port it shows while its firmware runs, as in [Diagnostic output](../reference/diagnostic-output.md#the-ear).

Every 2 seconds it prints a line like `ear alive 12 s, stage: prompt ready, rx 0 bytes`. If the stage stops before `prompt ready`, the ear did not finish starting, and the stage named is where it stopped.

The tests below pause the ear's frames to the eye while they run. The eye fades to silence and recovers by itself.

## 2. Does the microphone hear you?

Press `c`. The ear records for 3 seconds: clap, talk or play music close to the microphone while it does. Then it prints the level for each tenth of a second and its verdict:

- `the level tracked the noise - the mic hears`: the microphone works. If the patterns still do not react, the problem is between the ear and the eye; check the eye's `audio:` line in [Diagnostic output](../reference/diagnostic-output.md#every-5-seconds).
- `that is not sound...`: the level moved less than 20 dB. Go on to step 3.

## 3. Does any sound get through at all?

Press `f`. The ear measures a quiet second, then asks you to make noise, and compares each octave band. It is about 30 dB more sensitive than `c`.

- `a band rose while you made noise`: sound is getting through, but too weakly. Something may be partly blocking the microphone's sound hole, a tiny hole through the board under it.
- `no band rose`: nothing reaches it. Go on to step 4.

## 4. Is the microphone sending anything?

These check the connection to the microphone rather than whether it hears:

- Press `s`. It counts samples for a second and should report close to 48000 Hz. `nothing arriving at all` means the microphone's clock is not getting through; check its wiring, and see [How to change the microphone's pins](regenerate-the-mic-program.md) if it is on different pins.
- Press `t`. It prints statistics for a second, including a `bits` line and a verdict if the data line is stuck low, stuck high, or never changes.
- Press `r`. It prints the raw values with a guide: all `000000` means the data line is low or the microphone has no power; all `ffffff` means nothing is driving it; small values that change sign mean it is working.

If these all look normal but steps 2 and 3 found nothing, the microphone is sending but not hearing. Go on to step 5.

## 5. Is the microphone itself dead?

Press `c` again, and while it records, tap the board hard right beside the microphone with a fingernail. A working microphone picks that up as a big jump in the `peak` column. If the peak barely moves, the microphone's diaphragm, the part that moves with sound, is dead, even though its electronics still send data.

Replace the microphone. Its datasheet warns that washing the board, ultrasonic cleaning, blowing it off with air, or vacuum pickup on its bottom face can kill the diaphragm in exactly this way, so solder the replacement by hand rather than sending the board through cleaning again.

For what every key does, see [Mic diagnostic console](../../firmware/ear/README.md#mic-diagnostic-console).
