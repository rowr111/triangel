# How Triangel works

A Triangel is 25 triangle tiles of LEDs, two dabao boards that run them, and a few boards that connect everything. This page explains how those pieces work together. For the names of the parts, see [The parts](../../README.md#the-parts) on the front page.

## Two boards, two jobs

The controller board holds two dabao boards, each with its own Baochip-1x chip and its own firmware:

- The **eye** draws the patterns and sends them to the 600 LEDs. It also reads the buttons, the sound switch and the remote.
- The **ear** listens through the microphone, measures the sound in 24 frequency bands, from low notes to high, and sends the result to the eye over a single wire.

Splitting the work this way keeps the two jobs from getting in each other's way. Sending colors to 600 LEDs needs exact timing, and listening to sound means steady number crunching. On separate chips, neither one can delay the other.

The Baochip-1x runs at 350 MHz with 2 MB of memory and 4 MB of storage, and has no hardware for decimal-point math, which shapes how patterns are written; see [How to make a pattern fast enough](../how-to/make-a-pattern-fast-enough.md). Besides its main processor it has four small helper cores, its BIO cores: the eye uses two to send the LED data and one for the remote's receiver, and the ear uses one to clock the microphone.

## What the eye does each frame

The eye draws a new frame every 33 ms, about 30 times a second. Each time, it:

1. takes the newest sound from the ear,
2. handles any button or remote presses,
3. lets the setlist decide which pattern is on, and whether it is time to move to the next one,
4. asks that pattern for a color for each of the 600 LEDs, blending two patterns during a change,
5. applies the brightness setting,
6. sends the colors to the triangle tiles.

Sending takes about 11 ms. The triangle tiles are wired in two chains, and two of the chip's small helper cores, its BIO cores, clock the colors out to both chains at the same time. That about halves the time compared with one long chain. Whatever is left of the 33 ms is what a pattern has to work with; see [How to make a pattern fast enough](../how-to/make-a-pattern-fast-enough.md).

Built with the `previewer` option, the eye sends each frame over USB to your computer instead, and the [previewer](../how-to/run-the-previewer.md) draws it in a browser.

## Patterns and setlists

Every pattern is a small piece of code that fills in 600 colors when asked. The eye has two lists of them, called setlists:

- **ambient** patterns, which ignore sound and move slowly by themselves
- **sound** patterns, which react to music

The setlist moves to the next pattern every 3 minutes (ambient) or 5 minutes (sound), with a transition between them. The buttons and the remote can step through patterns, hold one, or change the brightness.

The sound switch picks the list: **Off** for ambient, **On** for sound, and **Auto**, which changes to the sound patterns once the room has had music for a while and back once it has been quiet for a while.

## What the ear sends

The ear measures the sound in 24 bands, each about a third of an octave wide. Each frame it sends covers 32 ms of sound. Along with the band levels, it sends how loud the sound is, both as a fixed level and compared with the music of the last little while.

The eye turns that into what the sound patterns use: band levels, which bands just jumped, the overall loudness, and the beats and drops it detects. See [Sound](../reference/pattern-toolkit.md#sound) in the Pattern toolkit for what a pattern gets.

## Buttons, switch and remote

The five buttons and the sound switch are on the input board, which connects to the eye over I2C, a two-wire connection for small chips. The IR receiver for the remote is there too. On the older all-in-one controller board, v1, the buttons connect to the eye directly instead, which is what the `combined-board` build option is for.

## Xous and the kernel

Both dabao boards run [Xous](https://betrusted.io/xous-book/), a small operating system written in Rust. Each board's firmware is a Xous program, and Xous itself, the kernel, is a separate pair of files flashed beside it.

Triangel uses its own build of the kernel for two reasons:

- The ear-to-eye wire uses a serial port that a normal Xous kernel keeps for its own log messages. Building the kernel with the `gdb-stub` option leaves it free.
- It includes a fix to the I2C driver, which the eye uses to read the input board, that is not yet in the main Xous code.

That is why the docs say to download the kernel files from Triangel's releases rather than letting the Baochip extension fetch standard ones. See [How to build the kernels](../how-to/build-the-kernels.md) to build them yourself.
