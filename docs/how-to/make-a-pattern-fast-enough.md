# How to make a pattern fast enough

The eye draws a frame every 33 ms, and about 11 ms of that goes to sending the frame to the LED chains. Everything a pattern does has to fit in what is left. The chip has no floating-point hardware, so every `f32` operation is a library call, and `sin`, `atan2`, `sqrt` and `powf` cost hundreds of cycles. One `sin` per LED is affordable; six is not.

Most of the work is in the loop over the 600 LEDs, so that is where to look. Paths here are relative to `firmware/eye/`.

## Work out fixed values once

Values that depend only on where an LED is belong in `led::geom`, or in a table the pattern builds in `new()`, not recalculated every frame. `led::geom` already has each LED's distance and angle from the center and its distance from the apex.

## Only visit the LEDs near a shape

For a shape that covers a small area, such as a ripple or a spark, use `led::grid` to visit only the LEDs near it, instead of testing every LED against every shape.

## Avoid a sin per LED

`sin(per_led + per_frame)` needs no `sin` per LED. A `Phasor` holds the `sin` and `cos` of an angle. Store one for the per-LED part in `new()`, make one for the frame's angle once per frame, and combine them with `sin_plus`. `phase_phasors()` gives phasors for offsets taken from a hash.

## Use whole numbers per LED

Where you can, do the per-LED math with whole numbers, as Shimmer and Flame do: values in Q12 (x4096 = 1.0), and lookup tables for curves and colors. A few `f32` calls per frame are fine.

## Wrap time before converting it

Wrap time to the period you need before converting it to `f32`, as in `(t_ms % PERIOD_MS) as f32`. Raw `t_ms` loses precision after hours of running.

## Checking your speed

In the previewer, watch the bridge's `frames/s:` line: if it drops below 30, the pattern is too slow. If it holds at right around 30 it's probably good enough, but check on a real Triangel too, since sending a frame to the LEDs takes a few ms longer than sending it to the previewer.
