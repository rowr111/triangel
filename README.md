# Triangel!

## A unique lighting fixture..

- 25 triangle-shaped tiles
- 600 LEDs! 
- ambient and sound-reactive patterns

## Why the name?

Each triangle tile has a stylized eye design in the middle.. After I decided to cover it in eyes, I was inspired/by biblical angel descriptions to name it..triangle + angel = triangel!


<img src="docs/images/triangel.jpg" width="480" alt="triangel: 25 triangle tiles joined into one large triangle, point down, with some of its LEDs lit">

## The parts

| Name | What it is |
|---|---|
| triangel | The whole project: the tiles and the boards that run them |
| tile | One of the 25 triangle circuit boards, each with 24 LEDs. |
| controller board | Holds the two dabao boards, the microphone and the power supply for the tiles |
| dabao board | A [dabao](https://www.crowdsupply.com/baochip/dabao), a Baochip-1x development board. triangel uses two: the **eye** and the **ear** |
| input board | The five buttons, the sound switch and the IR receiver |
| remote | A 7-button IR remote that does what the buttons do |

## How it works

The two dabao boards each run their own firmware under [Xous](https://betrusted.io/xous-book/):

- The **eye** draws the patterns and sends them to the 600 LEDs 30 times a second. It also reads the buttons, the switch and the remote.
- The **ear** listens through the microphone, measures the sound in 24 frequency bands, and sends the result to the eye.

## Where to go

- [Use an existing triangel](USER_GUIDE.md): the controls, and updating its firmware
- [Set up your machine](docs/how-to/set-up-a-development-machine.md)
- [Build and flash the boards](docs/how-to/build-and-flash.md)
- [Watch the patterns in a browser](docs/how-to/run-the-previewer.md)
- [Write a pattern](firmware/eye/PATTERNS.md)
- [Contributing](CONTRIBUTING.md)
- [Build the kernels](docs/how-to/build-the-kernels.md)
- [Change the microphone's pins](docs/how-to/regenerate-the-mic-program.md)
- [Print the stickers and overlays](graphics/README.md)

## What is in this repository

| Folder | Contents |
|---|---|
| [`firmware/eye/`](firmware/eye/README.md) | The eye's firmware: LED output, patterns, controls |
| [`firmware/ear/`](firmware/ear/README.md) | The ear's firmware: microphone, frequency bands, the link to the eye |
| [`firmware/shared/`](firmware/shared/README.md) | Code both use: the format of what the ear sends, and the sound tuning |
| [`triangel-previewer/`](triangel-previewer/) | Shows the eye's LED output in a browser |
| `hardware/` | The circuit boards (KiCad) and the brackets |
| [`graphics/`](graphics/README.md) | Artwork and the sticker sheets |

## License

MIT. See [LICENSE](LICENSE).
