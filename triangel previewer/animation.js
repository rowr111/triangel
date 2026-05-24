// animation.js - edit this file to define animations.
//
// getFrame(ledMap, t) is called every animation frame (~60fps).
//   ledMap : array of 600 {wx, wy, boardId, localIdx} - world positions in mm
//            origin top-left of big triangle; x right, y down
//            world spans ~0-517mm (x) x ~0-436mm (y), apex at bottom-center (~258, 436)
//   t      : DOMHighResTimeStamp (milliseconds, same as requestAnimationFrame)
//
// Returns: array of 600 [r, g, b] values, each 0-255.

// World bounds (approximate, based on gap-adjusted layout)
const WORLD_CX = 258;  // mm - horizontal center of installation
const WORLD_TOP = 6;   // mm - topmost LED y
const WORLD_BOT = 436; // mm - bottommost LED y
const WORLD_H   = WORLD_BOT - WORLD_TOP;

// --- Utilities ---

function hsv(h, s, v) {
  // h 0-360, s/v 0-1 -> [r, g, b] 0-255
  const f = n => { const k = (n + h / 60) % 6; return v - v * s * Math.max(0, Math.min(k, 4 - k, 1)); };
  return [f(5), f(3), f(1)].map(x => Math.round(x * 255));
}

function lerp(a, b, t) { return a + (b - a) * t; }
function clamp(x, lo, hi) { return Math.max(lo, Math.min(hi, x)); }

// --- Active animation ---

function getFrame(ledMap, t) {
  return ledMap.map(() => [0, 0, 0]);
}
