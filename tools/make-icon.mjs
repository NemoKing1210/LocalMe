// Throwaway generator for the LocalMe source icon. Produces a 1024x1024 PNG that
// `npx tauri icon` expands into the full platform icon set.
import { deflateSync } from 'node:zlib';
import { writeFileSync, mkdirSync } from 'node:fs';

const SIZE = 1024;
const SS = 2; // supersample factor for anti-aliasing
const W = SIZE * SS;

const px = new Float32Array(W * W * 4); // premultiplied rgb + a

const clamp01 = (v) => (v < 0 ? 0 : v > 1 ? 1 : v);

// Signed distance to a superellipse (approximate, good enough for an app icon).
function sdSquircle(x, y, r, n) {
  const ax = Math.abs(x / r);
  const ay = Math.abs(y / r);
  return (Math.pow(Math.pow(ax, n) + Math.pow(ay, n), 1 / n) - 1) * r;
}

function sdRoundRect(x, y, hw, hh, r) {
  const dx = Math.abs(x) - (hw - r);
  const dy = Math.abs(y) - (hh - r);
  const outside = Math.hypot(Math.max(dx, 0), Math.max(dy, 0));
  return outside + Math.min(Math.max(dx, dy), 0) - r;
}

function sdCircle(x, y, r) {
  return Math.hypot(x, y) - r;
}

function sdTriangle(px_, py, ax, ay, bx, by, cx, cy) {
  const d = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
  const s = Math.sign(d) || 1;
  const d1 = (bx - px_) * (ay - py) - (by - py) * (ax - px_);
  const d2 = (cx - bx) * (by - py) - (cy - by) * (bx - px_);
  const d3 = (ax - cx) * (cy - py) - (ay - cy) * (cx - px_);
  const inside = Math.min(s * d1, s * d2, s * d3) < 0;
  const edges = [
    [ax, ay, bx, by],
    [bx, by, cx, cy],
    [cx, cy, ax, ay],
  ];
  let best = Infinity;
  for (const [x1, y1, x2, y2] of edges) {
    const ex = x2 - x1;
    const ey = y2 - y1;
    const t = clamp01(((px_ - x1) * ex + (py - y1) * ey) / (ex * ex + ey * ey));
    best = Math.min(best, Math.hypot(px_ - (x1 + t * ex), py - (y1 + t * ey)));
  }
  return inside ? -best : best;
}

const lerp = (a, b, t) => a + (b - a) * t;

const rgb = (r, g, b) => [r / 255, g / 255, b / 255];

function blend(i, r, g, b, a) {
  if (a <= 0) return;
  const dstA = px[i + 3];
  const outA = a + dstA * (1 - a);
  if (outA <= 0) return;
  px[i] = (r * a + px[i] * dstA * (1 - a)) / outA;
  px[i + 1] = (g * a + px[i + 1] * dstA * (1 - a)) / outA;
  px[i + 2] = (b * a + px[i + 2] * dstA * (1 - a)) / outA;
  px[i + 3] = outA;
}

const coverage = (dist, pxPerUnit) => clamp01(0.5 - dist / pxPerUnit);

const cx = W / 2;
const cy = W / 2;
const outerR = W * 0.5 - W * 0.045;

// M3 primary tonal ramp: darker at the bottom, lighter at the top.
const topColor = rgb(0x8f, 0x77, 0xd0);
const bottomColor = rgb(0x4a, 0x33, 0x82);
const dotColor = rgb(0x2f, 0x22, 0x54);

for (let y = 0; y < W; y++) {
  for (let x = 0; x < W; x++) {
    const i = (y * W + x) * 4;
    const fx = x + 0.5 - cx;
    const fy = y + 0.5 - cy;

    const bg = sdSquircle(fx, fy, outerR, 4.2);
    const bgA = coverage(bg, 1);
    if (bgA <= 0) continue;
    const t = 0.05 + (y / W) * 0.9;
    blend(
      i,
      lerp(bottomColor[0], topColor[0], t),
      lerp(bottomColor[1], topColor[1], t),
      lerp(bottomColor[2], topColor[2], t),
      bgA,
    );

    // Speech bubble: a rounded rectangle with a round nub at its lower left.
    const bw = W * 0.255;
    const bh = W * 0.185;
    const bubbleY = -W * 0.05;
    const bodyD = sdRoundRect(fx, fy - bubbleY, bw, bh, W * 0.075);
    const nubD = sdCircle(fx + bw * 0.62, fy - (bubbleY + bh * 0.92), W * 0.072);
    const bubbleD = Math.min(bodyD, nubD);
    blend(i, 1, 1, 1, coverage(bubbleD, 1) * 0.98);

    // Three dots inside the bubble.
    const dotR = W * 0.032;
    const dotGap = W * 0.082;
    for (const dx of [-dotGap, 0, dotGap]) {
      const dotA = coverage(sdCircle(fx - dx, fy - bubbleY, dotR), 1);
      blend(i, dotColor[0], dotColor[1], dotColor[2], dotA);
    }
  }
}

// Downsample with box filtering.
const out = Buffer.alloc(SIZE * SIZE * 4);
for (let y = 0; y < SIZE; y++) {
  for (let x = 0; x < SIZE; x++) {
    let r = 0;
    let g = 0;
    let b = 0;
    let a = 0;
    for (let sy = 0; sy < SS; sy++) {
      for (let sx = 0; sx < SS; sx++) {
        const i = ((y * SS + sy) * W + (x * SS + sx)) * 4;
        r += px[i];
        g += px[i + 1];
        b += px[i + 2];
        a += px[i + 3];
      }
    }
    const n = SS * SS;
    const o = (y * SIZE + x) * 4;
    out[o] = Math.round((r / n) * 255);
    out[o + 1] = Math.round((g / n) * 255);
    out[o + 2] = Math.round((b / n) * 255);
    out[o + 3] = Math.round((a / n) * 255);
  }
}

// PNG encoding.
const raw = Buffer.alloc((SIZE * 4 + 1) * SIZE);
for (let y = 0; y < SIZE; y++) {
  raw[y * (SIZE * 4 + 1)] = 0; // filter: none
  out.copy(raw, y * (SIZE * 4 + 1) + 1, y * SIZE * 4, (y + 1) * SIZE * 4);
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, 'ascii'), data]);
  const crcTable = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    crcTable[n] = c >>> 0;
  }
  let crc = 0xffffffff;
  for (const byte of body) crc = crcTable[(crc ^ byte) & 0xff] ^ (crc >>> 8);
  const crcBuf = Buffer.alloc(4);
  crcBuf.writeUInt32BE((crc ^ 0xffffffff) >>> 0, 0);
  return Buffer.concat([len, body, crcBuf]);
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk('IHDR', ihdr),
  chunk('IDAT', deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);

mkdirSync('src-tauri/icons', { recursive: true });
writeFileSync('src-tauri/icons/app-icon.png', png);
console.log(`wrote src-tauri/icons/app-icon.png (${png.length} bytes)`);
