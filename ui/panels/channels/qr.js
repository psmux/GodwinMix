// A QR code encoder, small enough to own: byte mode, error correction level M,
// versions 1 to 10, which is up to 213 bytes and more than any publish address
// with a key on the end. Written from the ISO 18004 layout, in the shape of
// Project Nayuki's reference encoder. Nothing is fetched from anywhere.
//
// Only the channels page uses it, to put an encoder's address on a phone.

import { Grid } from "./qr-grid.js";

/** Error correction codewords per block, and the block count, for level M. */
const ECC_M = [10, 16, 26, 18, 24, 16, 18, 22, 22, 26];
const BLOCKS_M = [1, 1, 1, 2, 2, 4, 4, 4, 5, 5];
const MAX_VERSION = 10;

/**
 * The modules of a QR code for `text`, as rows of booleans (true is dark), or
 * null when it does not fit in version 10.
 */
export function qrMatrix(text) {
  const bytes = [...new TextEncoder().encode(String(text))];
  for (let ver = 1; ver <= MAX_VERSION; ver++) {
    const capacity = dataCodewords(ver) * 8;
    const used = 4 + (ver < 10 ? 8 : 16) + bytes.length * 8;
    if (used <= capacity) return build(ver, bytes);
  }
  return null;
}

/** The same code as an SVG path in module units, with the quiet zone in the viewBox. */
export function qrPath(text) {
  const m = qrMatrix(text);
  if (!m) return null;
  let d = "";
  m.forEach((row, y) => row.forEach((dark, x) => { if (dark) d += `M${x + 4} ${y + 4}h1v1h-1z`; }));
  return { d, size: m.length + 8 };
}

function rawModules(ver) {
  let n = (16 * ver + 128) * ver + 64;
  if (ver >= 2) {
    const align = Math.floor(ver / 7) + 2;
    n -= (25 * align - 10) * align - 55;
    if (ver >= 7) n -= 36;
  }
  return n;
}

function dataCodewords(ver) {
  return Math.floor(rawModules(ver) / 8) - ECC_M[ver - 1] * BLOCKS_M[ver - 1];
}

function build(ver, bytes) {
  const bits = [];
  const put = (value, len) => { for (let i = len - 1; i >= 0; i--) bits.push((value >>> i) & 1); };
  put(4, 4);
  put(bytes.length, ver < 10 ? 8 : 16);
  for (const b of bytes) put(b, 8);
  const capacity = dataCodewords(ver) * 8;
  put(0, Math.min(4, capacity - bits.length));
  put(0, (8 - (bits.length % 8)) % 8);
  const data = [];
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((a, b) => (a << 1) | b, 0));
  for (let pad = 0xec; data.length < capacity / 8; pad ^= 0xec ^ 0x11) data.push(pad);

  const size = ver * 4 + 17;
  const grid = new Grid(size);
  grid.functions(ver);
  grid.codewords(interleave(ver, data));
  let best = null;
  for (let mask = 0; mask < 8; mask++) {
    const trial = grid.copy();
    trial.mask(mask);
    trial.format(mask);
    const score = trial.penalty();
    if (!best || score < best.score) best = { score, trial };
  }
  return best.trial.dark;
}

function interleave(ver, data) {
  const blocks = BLOCKS_M[ver - 1];
  const eccLen = ECC_M[ver - 1];
  const raw = Math.floor(rawModules(ver) / 8);
  const short = blocks - (raw % blocks);
  const shortLen = Math.floor(raw / blocks);
  const divisor = rsDivisor(eccLen);
  const out = [];
  let k = 0;
  const made = [];
  for (let i = 0; i < blocks; i++) {
    const dat = data.slice(k, k + shortLen - eccLen + (i < short ? 0 : 1));
    k += dat.length;
    const ecc = rsRemainder(dat, divisor);
    if (i < short) dat.push(0);
    made.push(dat.concat(ecc));
  }
  for (let i = 0; i < made[0].length; i++) {
    for (let j = 0; j < made.length; j++) {
      if (i !== shortLen - eccLen || j >= short) out.push(made[j][i]);
    }
  }
  return out;
}

function gfMul(x, y) {
  let z = 0;
  for (let i = 7; i >= 0; i--) {
    z = (z << 1) ^ ((z >>> 7) * 0x11d);
    z ^= ((y >>> i) & 1) * x;
  }
  return z & 0xff;
}

function rsDivisor(degree) {
  const result = new Array(degree).fill(0);
  result[degree - 1] = 1;
  let root = 1;
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      result[j] = gfMul(result[j], root);
      if (j + 1 < degree) result[j] ^= result[j + 1];
    }
    root = gfMul(root, 2);
  }
  return result;
}

function rsRemainder(data, divisor) {
  const result = divisor.map(() => 0);
  for (const b of data) {
    const factor = b ^ result.shift();
    result.push(0);
    divisor.forEach((coef, i) => { result[i] ^= gfMul(coef, factor); });
  }
  return result;
}

