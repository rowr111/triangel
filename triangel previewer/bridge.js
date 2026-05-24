// bridge.js — serial-to-WebSocket bridge for the Triangel previewer.
//
// Usage:
//   node bridge.js
//   node bridge.js --serial COM4 --baud 460800 --ws-port 9000
//
// Config file bridge.config.json is read first; CLI args override it.
// The Baochip sends 1804 bytes per frame: 4-byte magic + 1800 RGB bytes (600 LEDs × RGB) in chain order.
// Magic [0xFF, 0xFF, 0xFF, 0xFF] allows the bridge to sync to frame boundaries mid-stream.
// LED channel values are clamped to 0–254 in firmware so 0xFF never appears in payload.

'use strict';

const fs   = require('fs');
const path = require('path');
const { SerialPort }      = require('serialport');
const { WebSocketServer } = require('ws');

// --- Config: file + CLI overrides ----------------------------------------

const configPath = path.join(__dirname, 'bridge.config.json');
const fileConfig = fs.existsSync(configPath)
  ? JSON.parse(fs.readFileSync(configPath, 'utf8'))
  : {};

const argv = process.argv.slice(2);
function cliArg(name) {
  const i = argv.indexOf('--' + name);
  return i !== -1 ? argv[i + 1] : undefined;
}

const SERIAL_PATH  = cliArg('serial')  ?? fileConfig.serial  ?? 'COM3';
const BAUD_RATE    = parseInt(cliArg('baud')    ?? fileConfig.baud    ?? 921600);
const WS_PORT      = parseInt(cliArg('ws-port') ?? fileConfig.wsPort  ?? 8080);
const FRAME_MAGIC  = Buffer.from([0xFF, 0xFF, 0xFF, 0xFF]); // must match MAGIC in led/mod.rs
const FRAME_SIZE   = 600 * 3;                                // 1800 bytes payload
const PACKET_SIZE  = FRAME_MAGIC.length + FRAME_SIZE;        // 1804 bytes on the wire

console.log(`Config: serial=${SERIAL_PATH}  baud=${BAUD_RATE}  ws-port=${WS_PORT}`);

// --- WebSocket server -----------------------------------------------------

const wss = new WebSocketServer({ port: WS_PORT });
console.log(`WebSocket listening on ws://localhost:${WS_PORT}`);

wss.on('connection', () => {
  console.log(`Browser connected (${wss.clients.size} client(s))`);
});

function broadcast(data) {
  for (const client of wss.clients) {
    if (client.readyState === client.OPEN) client.send(data);
  }
}

// --- Serial port ----------------------------------------------------------

let buf = Buffer.alloc(0);
let frameCount = 0;
setInterval(() => {
  if (frameCount > 0) console.log(`frames/s: ${frameCount}`);
  frameCount = 0;
}, 1000);

const serial = new SerialPort({ path: SERIAL_PATH, baudRate: BAUD_RATE });

serial.on('open', () => {
  console.log(`Serial open: ${SERIAL_PATH} @ ${BAUD_RATE} baud`);
});

serial.on('data', (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  while (true) {
    const idx = buf.indexOf(FRAME_MAGIC);
    if (idx === -1) {
      // No magic found — keep last 3 bytes in case the marker is split across chunks.
      buf = buf.subarray(buf.length - (FRAME_MAGIC.length - 1));
      break;
    }
    if (idx > 0) buf = buf.subarray(idx);
    if (buf.length < PACKET_SIZE) break;   // wait for a full packet
    broadcast(buf.subarray(FRAME_MAGIC.length, PACKET_SIZE)); // strip magic, send 1800 bytes
    buf = buf.subarray(PACKET_SIZE);
    frameCount++;
  }
});

serial.on('error', (err) => {
  console.error('Serial error:', err.message);
});
