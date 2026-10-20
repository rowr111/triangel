# Triangel!

## A unique lighting fixture..

- 25 triangle tiles
- 600 LEDs! 
- ambient and sound-reactive patterns

## Why the name?

Each triangle tile has a stylized eye design in the middle.. After I decided to cover it in eyes, I was inspired/by biblical angel descriptions to name it..triangle + angel = triangel!


<img src="docs/images/triangel.jpg" width="480" alt="triangel: 25 triangle tiles joined into one large triangle, point down, with some of its LEDs lit">

## The parts

| Name | What it is |
|---|---|
| triangel | The whole project: the triangle tiles and the boards that run them |
| triangle tile | One of the 25 triangle circuit boards, each with 24 LEDs. |
| controller board | Holds the two dabao boards, the microphone and the power supply for the triangle tiles |
| dabao board | A [dabao](https://www.crowdsupply.com/baochip/dabao), a Baochip-1x development board. triangel uses two: the **eye** and the **ear** |
| input board | The five buttons, the sound switch and the IR receiver |
| remote | A 7-button IR remote that does what the buttons do |

## How it works

The **eye** draws the patterns and the **ear** listens to the room. See [How triangel works](docs/explanation/how-triangel-works.md).

## Where to go

### If you have a triangel

- [Use your triangel](USER_GUIDE.md#controls): the controls
- [Update its firmware](docs/how-to/update-your-triangel.md)
- [Make a dabao start by itself](docs/how-to/make-a-dabao-start-by-itself.md), after replacing one
- [Print the stickers and overlays](graphics/README.md)

### If you want to develop

#### Learn

- [Try it on one dabao](docs/tutorials/first-run.md): start here if you are new
- [Write your first pattern](docs/tutorials/first-pattern.md)
- [Write a sound pattern](docs/tutorials/first-sound-pattern.md)

#### How to

- [Set up your machine](docs/how-to/set-up-a-development-machine.md)
- [Build and flash the boards](docs/how-to/build-and-flash.md)
- [Make a dabao start by itself](docs/how-to/make-a-dabao-start-by-itself.md)
- [Watch the patterns in a browser](docs/how-to/run-the-previewer.md)
- [Add a pattern](docs/how-to/add-a-pattern.md)
- [Make a pattern fast enough](docs/how-to/make-a-pattern-fast-enough.md)
- [Build the kernels](docs/how-to/build-the-kernels.md)
- [Change the microphone's pins](docs/how-to/regenerate-the-mic-program.md)
- [Regenerate the LED map](docs/how-to/regenerate-the-led-map.md)

#### Understand

- [How triangel works](docs/explanation/how-triangel-works.md): the two boards, a frame, the setlists and the kernel

#### Look things up

- [Pattern toolkit](docs/reference/pattern-toolkit.md): the helpers, the measurements and the sound
- [Triangle tiles and wiring](docs/reference/triangle-tiles-and-wiring.md): the layout, the numbering and the two chains
- [Contributing](CONTRIBUTING.md)

## What is in this repository

| Folder | Contents |
|---|---|
| [`firmware/eye/`](firmware/eye/README.md) | The eye's firmware: LED output, patterns, controls |
| [`firmware/ear/`](firmware/ear/README.md) | The ear's firmware: microphone, frequency bands, the link to the eye |
| [`firmware/shared/`](firmware/shared/README.md) | Code both use: the format of what the ear sends, and the sound tuning |
| [`triangel-previewer/`](triangel-previewer/) | Shows the eye's LED output in a browser |
| [`hardware/`](hardware/) | The circuit boards (KiCad) and the brackets |
| [`graphics/`](graphics/README.md) | Artwork and the sticker sheets |

## License

MIT. See [LICENSE](LICENSE).
