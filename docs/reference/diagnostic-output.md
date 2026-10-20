# Diagnostic output

The status lines the eye and the ear print over USB serial, which help with troubleshooting. Both print on the serial port the dabao shows while its firmware runs, not the one it shows in bootloader mode. Open it with a terminal program, or with the [Baochip sidebar](../images/baochip-sidebar.png): click **Set run mode serial port** and pick the dabao's port, make sure **Default monitor** says **Run**, then click **Monitor**.

An open serial monitor slows the eye down a little, so do not judge a pattern's speed while one is attached.

## The eye

The eye prints only when built with the `bringup` option, and not together with `previewer`, whose serial stream carries LED frames instead. See [How to build the firmware and flash the boards](../how-to/build-and-flash.md).

### Boot

As it starts, the eye prints each stage it reaches:

```
eye boot: main start
eye boot: led out up
eye boot: audio up
eye boot: input spawned
eye boot: render loop
```

`render loop` means it is drawing frames. If it stops earlier, the last stage printed says where. Around these come `heartbeat thread up`, and lines from the remote's receiver:

| Line | Meaning |
|---|---|
| `IR: thread start, initializing BIO capture` | The remote's receiver is starting |
| `IR capture on BIO pin N at N Hz` | It is ready |
| `IR NecCapture init FAILED: ...` | It could not start. The buttons still work |

### Audio link start

The eye retries setting up the link from the ear:

| Line | Meaning |
|---|---|
| `audio UART init failed (status N); retrying` | A try failed; the status is from the table below |
| `audio UART init recovered after N retries` | It worked after retrying |
| `audio UART init failed after N attempts; running with no audio` | It gave up. The patterns run, but nothing reacts to sound |

### Every 5 seconds

When anything about the audio link has changed since the last time, the eye prints two lines:

```
audio: status 5, frames 1234, dropped 0, first byte aa, -32.0 dBFS, norm 0.41
drop: normal, drops 2, bass -28.0 dB against -30.5 dB
```

- **status:** the state of the link:

  | Status | Meaning |
  |---|---|
  | 0 | Not set up yet |
  | 1, 2 | Setting up the link failed |
  | 3 | Set up, but nothing has arrived from the ear |
  | 4 | Bytes are arriving, but none of them make a valid frame |
  | 5 | Frames are arriving: the link works |

- **frames:** frames received; **dropped:** frames thrown away as damaged
- **first byte:** the first byte that ever arrived, in hex. Normally `aa`, the marker that starts every frame from the ear
- **dBFS:** the latest loudness from the ear; **norm:** the same, compared with recent music, from 0 to 1
- **drop:** `normal`, or `in breakdown` while the bass has gone quiet; **drops:** drops detected so far; then the current bass level against the level it is compared with

Then, until boot reaches the render loop, and after that only when the remote's counters change:

```
alive 125 s, stage: render loop, ir: decoded 3, rejected 0, clock N Hz, last frame N
```

- **alive:** seconds since start; **stage:** the last boot stage reached
- **ir decoded / rejected:** remote presses understood, and signals thrown away
- **clock:** the remote receiver's timing clock; **last frame:** the last signal received, in hex

### Remote presses

| Line | Meaning |
|---|---|
| `IR frame ok: cmd N (frame N)` | A press from Triangel's remote, with its button code |
| `IR frame rejected: N` | A signal that is not from Triangel's remote, or was garbled |

## The ear

The ear always prints; it has no build option for it.

As it starts, it prints each stage:

```
stage: usb up
stage: hal up
stage: starting BIO core
stage: BIO core up
stage: prompt ready
```

`prompt ready` means it is listening and sending frames to the eye. Until you press a key, it also prints every 2 seconds:

```
ear alive 12 s, stage: prompt ready, rx 0 bytes
```

with the last stage reached and how many bytes it has received from you. After `mic>`, single keys run tests on the microphone; see [Mic diagnostic console](../../firmware/ear/README.md#mic-diagnostic-console) for the keys.

## The previewer stream

With `previewer`, the eye's serial port carries LED frames instead of text: a 4-byte marker, `FF FF FF FF`, then one red, green and blue byte for each LED in order. Colors are capped at 254, so the marker never appears inside a frame.
