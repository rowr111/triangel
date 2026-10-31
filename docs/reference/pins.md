# Pins

Which dabao pins each board uses. The firmware's own lists are `firmware/eye/src/pins.rs` and `firmware/ear/src/pins.rs`; if this page and those files disagree, the files are right.

## The eye

| Pin | Use |
|---|---|
| PB4 | LED data, chain 1 (triangle tiles 1-12, 288 LEDs) |
| PB5 | LED data, chain 2 (triangle tiles 13-25, 312 LEDs) |
| PB13 | Receives the frames from the ear (UART2 RX) |
| PB11, PB12 | I2C to the input board |
| PC7 | The input board's interrupt line: it goes low when a button or the switch changes |
| PC8 | The IR receiver for the remote |

The LED data and the IR receiver are driven by the chip's BIO helper cores, which number the pins differently: PB0-PB15 are BIO 0-15 and PC0-PC15 are BIO 16-31, so PC8 is BIO 24.

### The input board

The buttons and the sound switch are read through an MCP23008 chip on the input board, at I2C address 0x20. Each input is one bit, low when pressed or selected:

| Bit | Input |
|---|---|
| 0 | Sound switch, Off |
| 1 | Sound switch, On |
| 2 | Right |
| 3 | Down |
| 4 | Center |
| 5 | Left |
| 6 | Up |
| 7 | Not used |

In the switch's Auto position, neither switch bit is low.

### The all-in-one controller boards (v1 and v3)

With the `combined-board` build option, the buttons and switch are wired to the eye directly instead. The firmware's pins follow v3, shown here. On v1, Up and Down are the other way round (PC0 is Up, PC7 is Down), the LED data pins are PB4 for chain 1 and PB5 for chain 2, and the sound switch is on PB3 (On) and PB2 (Off), so a v1 build needs those changed back in `firmware/eye/src/pins.rs`.

| Pin | Use |
|---|---|
| PC10 | LED data, chain 1 (triangle tiles 1-12, 288 LEDs) |
| PC9 | LED data, chain 2 (triangle tiles 13-25, 312 LEDs) |
| PC7 | Up |
| PC0 | Down |
| PC3 | Left |
| PC1 | Right |
| PC2 | Center |
| PC12 | Sound switch, On |
| PC11 | Sound switch, Off |

## The ear

| Pin | Use |
|---|---|
| PB14 | Sends the frames to the eye (UART2 TX), one wire to the eye's PB13 |
| PB1 | Microphone clock (BCLK) |
| PB2 | Microphone data (SD) |
| PB3 | Microphone word select (WS) |

The microphone is an ICS43434 (JLCPCB part C5656610). Its channel select pin is tied low on the board, so it sends on the left channel, which is the one the ear reads.

The microphone's pins are built into a small program for a BIO core, so moving them takes more than changing a number; see [How to change the microphone's pins](../how-to/regenerate-the-mic-program.md). Keep them off PB11 and PB12, which are the I2C pins.
