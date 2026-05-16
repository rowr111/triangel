// animation.js — edit this file to define animations.
//
// getFrame(ledMap, t) is called every animation frame (~60fps).
//   ledMap : array of 600 {wx, wy, boardId, localIdx} — world positions in mm
//            origin top-left of big triangle; x right, y down
//            world spans ~0–517mm (x) × ~0–436mm (y), apex at bottom-center (~258, 436)
//   t      : DOMHighResTimeStamp (milliseconds, same as requestAnimationFrame)
//
// Returns: array of 600 [r, g, b] values, each 0–255.

// World bounds (approximate, based on gap-adjusted layout)
const WORLD_CX = 258;  // mm — horizontal center of installation
const WORLD_TOP = 6;   // mm — topmost LED y
const WORLD_BOT = 436; // mm — bottommost LED y
const WORLD_H   = WORLD_BOT - WORLD_TOP;

// ─── Utilities ────────────────────────────────────────────────────────────────

function hsv(h, s, v) {
  // h 0–360, s/v 0–1 → [r, g, b] 0–255
  const f = n => { const k = (n + h / 60) % 6; return v - v * s * Math.max(0, Math.min(k, 4 - k, 1)); };
  return [f(5), f(3), f(1)].map(x => Math.round(x * 255));
}

function lerp(a, b, t) { return a + (b - a) * t; }
function clamp(x, lo, hi) { return Math.max(lo, Math.min(hi, x)); }

// ─── Animations ───────────────────────────────────────────────────────────────

// Static white — geometry check
function staticWhite(ledMap) {
  return ledMap.map(() => [200, 200, 200]);
}

// Rainbow by x position
function rainbowX(ledMap, t) {
  const speed = 60; // mm/s
  const offset = (t / 1000) * speed;
  return ledMap.map(({ wx }) => {
    const hue = ((wx + offset) / 517 * 360) % 360;
    return hsv(hue, 1, 1);
  });
}

// Ripple from apex (bottom-center) upward
function apexRipple(ledMap, t) {
  const APEX_X = 258, APEX_Y = 436;
  const speed = 100; // mm/s
  const wavelength = 80; // mm
  return ledMap.map(({ wx, wy }) => {
    const dist = Math.sqrt((wx - APEX_X) ** 2 + (wy - APEX_Y) ** 2);
    const phase = (dist - (t / 1000) * speed) / wavelength * Math.PI * 2;
    const brightness = (Math.sin(phase) + 1) / 2;
    return [Math.round(brightness * 255), Math.round(brightness * 100), 0];
  });
}

// Horizontal scan (frequency-band style)
function horizontalScan(ledMap, t) {
  const period = 2000; // ms
  const scanY = WORLD_TOP + ((t % period) / period) * WORLD_H;
  const bandwidth = 30; // mm
  return ledMap.map(({ wy }) => {
    const dist = Math.abs(wy - scanY);
    const brightness = Math.max(0, 1 - dist / bandwidth);
    return [0, Math.round(brightness * 200), Math.round(brightness * 255)];
  });
}

// ─── Active animation ─────────────────────────────────────────────────────────

function getFrame(ledMap, t) {
  return apexRipple(ledMap, t);
  //return rainbowX(ledMap, t);
  //return horizontalScan(ledMap, t);
  // return staticWhite(ledMap);
}
