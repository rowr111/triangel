# How to run the previewer

The previewer shows triangel's 600 LEDs in a browser, live from an eye on your desk. It needs [Node.js](https://nodejs.org).

## 1. Flash the eye with `previewer`

Build and flash the eye with the `previewer` option, as in [How to build and flash the boards](build-and-flash.md). A bare dabao on a USB cable is enough.

## 2. Install the packages

Once, from `triangel-previewer/`:

```
npm install
```

## 3. Set the serial port

A running dabao shows up as a serial port. Put its name in `triangel-previewer/bridge.config.json`:

```json
{
  "serial": "COM3",
  "baud": 921600
}
```

Port names look like this:

| System | Name |
|---|---|
| Windows | `COM3`, `COM10` and so on |
| Linux | `/dev/ttyACM0` |
| macOS | `/dev/cu.usbmodem...` |

The dabao shows two serial ports: one while it is in its bootloader and one while its firmware runs. The previewer needs the second.

On Linux, your user needs to be in the `dialout` group to open the port: `sudo usermod -aG dialout $USER`, then log out and back in.

## 4. Start the bridge

From `triangel-previewer/`:

```
node bridge.js
```

It prints `Serial open:` with the port name once it reaches the eye, and `frames/s:` with a count once frames arrive.

The port can also be given on the command line, without editing the file: `node bridge.js --serial /dev/ttyACM0`.

## 5. Open the page

Open `triangel-previewer/index.html` in a browser. The patterns appear, and the buttons in the lower-left corner work like the ones on a triangel.

The corner of the page says `bridge live` while frames arrive, `no frames` when the bridge is running but the eye is not sending, and `no bridge` when the page cannot reach the bridge.
