# How to make a pattern fast enough

The eye draws a frame every 33 ms, and about 11 ms of that goes to sending the frame to the LED chains. Everything a pattern does has to fit in what is left. The chip has no floating-point hardware, so every `f32` operation is a library call, and `sin`, `atan2`, `sqrt` and `powf` cost hundreds of cycles. One `sin` per LED is affordable; six is not.

Most of the work is in the loop over the 600 LEDs, so that is where to look. Paths here are relative to `firmware/eye/`.

## Work out fixed values once

Values that depend only on where an LED is belong in `led::geom`, or in a table the pattern builds in `new()`, not recalculated every frame. `led::geom` already has each LED's distance and angle from the center and its distance from the apex.

## Only visit the LEDs near a shape

For a shape that covers a small area, such as a ripple or a spark, use `led::grid` to visit only the LEDs near it, instead of testing every LED against every shape.

## Avoid a sin per LED

`sin(per_led + per_frame)` needs no `sin` per LED. Store `sin` and `cos` of the per-LED part, take `sin` and `cos` of the frame's angle once, and combine them with `sin_sum`. `phase_phasors()` covers offsets taken from a hash.

## Use whole numbers per LED

Where you can, do the per-LED math with whole numbers, as Shimmer and Flame do: values in Q12 (x4096 = 1.0), and lookup tables for curves and colors. A few `f32` calls per frame are fine.

## Wrap time before converting it

Wrap time to the period you need before converting it to `f32`, as in `(t_ms % PERIOD_MS) as f32`. Raw `t_ms` loses precision after hours of running.

## Check it on a real triangel

The previewer caps at 30 frames a second and runs a few ms faster than the real triangel, so it can show that a pattern is too slow but cannot confirm that one is fast enough. Watch the pattern on a real triangel: a pattern that is too slow stutters.
