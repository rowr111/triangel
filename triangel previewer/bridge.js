// bridge.js — serial-to-WebSocket bridge for the Triangel previewer.
//
// Usage:
//   node bridge.js
//   node bridge.js --serial COM4 --baud 460800 --ws-port 9000
//
// Config file bridge.config.json is read first; CLI args override it.
// The Baochip sends 1800 bytes per frame (600 LEDs × RGB) in chain order.

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

const SERIAL_PATH = cliArg('serial')  ?? fileConfig.serial  ?? 'COM3';
const BAUD_RATE   = parseInt(cliArg('baud')    ?? fileConfig.baud    ?? 921600);
const WS_PORT     = parseInt(cliArg('ws-port') ?? fileConfig.wsPort  ?? 8080);
const FRAME_SIZE  = 600 * 3; // 1800 bytes — one RGB byte-triple per LED

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

const serial = new SerialPort({ path: SERIAL_PATH, baudRate: BAUD_RATE });

serial.on('open', () => {
  console.log(`Serial open: ${SERIAL_PATH} @ ${BAUD_RATE} baud`);
});

serial.on('data', (chunk) => {
  buf = Buffer.concat([buf, chunk]);
  // Emit complete frames as they arrive; discard any partial leading bytes.
  while (buf.length >= FRAME_SIZE) {
    broadcast(buf.subarray(0, FRAME_SIZE));
    buf = buf.subarray(FRAME_SIZE);
  }
});

serial.on('error', (err) => {
  console.error('Serial error:', err.message);
});
