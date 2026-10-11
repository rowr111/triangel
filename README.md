# triangel

A ceiling light shaped like a triangle about 50 cm across, made to sit in the corner of a room. It is built from 25 triangle tiles carrying 600 LEDs, and runs ambient and sound-reactive patterns.

<img src="docs/images/triangel.jpg" width="480" alt="triangel: 25 triangle tiles joined into one large triangle, point down, with some of its LEDs lit">

To work on triangel you need DABAO boards. Every page here assumes you have them.

## The parts

| Name | What it is |
|---|---|
| triangel | The whole project: the tiles and the boards that run them |
| tile | One of the 25 triangle circuit boards, with 24 LEDs. Also called a triangel. The code calls a tile a `board` |
| controller board | Holds the two DABAO boards, the microphone and the power supply for the tiles |
| DABAO board | A Baochip-1x development board. triangel uses two: the **eye** and the **ear** |
| input board | The five buttons, the sound switch and the IR receiver |
| remote | A 7-button IR remote that does what the buttons do |

## How it works

The two DABAO boards each run their own firmware under [Xous](https://betrusted.io/xous-book/):

- The **eye** draws the patterns and sends them to the 600 LEDs 30 times a second. It also reads the buttons, the switch and the remote.
- The **ear** listens through the microphone, measures the sound in 24 frequency bands, and sends the result to the eye about 28 times a second.

## Where to go

| If you want to | Read |
|---|---|
| Use a triangel: the controls, updating its firmware | [USER_GUIDE.md](USER_GUIDE.md) |
| Set up your machine and build the firmware | [firmware/README.md](firmware/README.md) |
| Watch the patterns in a browser | [triangel previewer/README.md](triangel%20previewer/README.md) |
| Write a pattern | [firmware/eye/PATTERNS.md](firmware/eye/PATTERNS.md) |
| Know the checks and conventions before changing something | [CONTRIBUTING.md](CONTRIBUTING.md) |
| Build the kernels the boards run | [docs/how-to/build-the-kernels.md](docs/how-to/build-the-kernels.md) |
| Change the microphone's pins | [docs/how-to/regenerate-the-mic-program.md](docs/how-to/regenerate-the-mic-program.md) |
| Print the stickers and overlays | [graphics/README.md](graphics/README.md) |

## What is in this repository

| Folder | Contents |
|---|---|
| [`firmware/eye/`](firmware/eye/README.md) | The eye's firmware: LED output, patterns, controls |
| [`firmware/ear/`](firmware/ear/README.md) | The ear's firmware: microphone, frequency bands, the link to the eye |
| [`firmware/shared/`](firmware/shared/README.md) | Code both use: the format of what the ear sends, and the sound tuning |
| [`triangel previewer/`](triangel%20previewer/README.md) | Shows the eye's LED output in a browser |
| `hardware/` | The circuit boards (KiCad) and the brackets |
| [`graphics/`](graphics/README.md) | Artwork and the sticker sheets |

## License

MIT. See [LICENSE](LICENSE).
