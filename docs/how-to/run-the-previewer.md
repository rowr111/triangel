# How to run the previewer

The previewer shows Triangel's 600 LEDs in a browser, live from an eye on your desk. It needs [Node.js](https://nodejs.org).

## 1. Flash the eye with `previewer`

Build and flash the eye with the `previewer` option, as in [How to build the firmware and flash the boards](build-and-flash.md). A bare dabao on a USB cable is enough.

## 2. Install the packages

Once, from `triangel-previewer/`:

```
npm install
```

## 3. Find the serial port

A running dabao shows up as a serial port. In VS Code, **Set run mode serial port** in the [Baochip sidebar](../images/baochip-sidebar.png) lists it. Port names look like this:

| System | Name |
|---|---|
| Windows | `COM3`, `COM10` and so on |
| Linux | `/dev/ttyACM0` |
| macOS | `/dev/cu.usbmodem...` |

The dabao shows two serial ports: one while it is in bootloader mode and one while its firmware runs. The previewer needs the second.

On Linux, your user needs to be in the `dialout` group to open the port: `sudo usermod -aG dialout $USER`, then log out and back in.

## 4. Start the bridge

From `triangel-previewer/`, with your port:

```
node bridge.js --serial COM10
```

It prints `Serial open:` with the port name once it reaches the eye, and `frames/s:` with a count once frames arrive.

To skip `--serial` each time, create `triangel-previewer/bridge.config.json` with your port; then `node bridge.js` is enough:

```json
{
  "serial": "COM10"
}
```

## 5. Open the page

Open `triangel-previewer/index.html` in a browser. The patterns appear, and the buttons in the lower-left corner work like the ones on a triangel.

The corner of the page says `bridge live` while frames arrive, `no frames` when the bridge is running but the eye is not sending, and `no bridge` when the page cannot reach the bridge.

## If port 8080 is taken

The bridge and the page talk over port 8080. If another program already uses it, the bridge fails to start. Pick another port for both: start the bridge with `--ws-port`, and add the same port to the page's address:

```
node bridge.js --serial COM10 --ws-port 9000
```

Then open `triangel-previewer/index.html?port=9000`.
