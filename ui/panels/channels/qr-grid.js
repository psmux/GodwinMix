// The module grid of a QR code: the fixed patterns, the zigzag the data is
// laid in, the eight masks and the score that picks one. qr.js does the bytes.

export class Grid {
  constructor(size) {
    this.size = size;
    this.dark = Array.from({ length: size }, () => new Array(size).fill(false));
    this.fixed = Array.from({ length: size }, () => new Array(size).fill(false));
  }

  copy() {
    const g = new Grid(this.size);
    g.dark = this.dark.map((r) => r.slice());
    g.fixed = this.fixed;
    return g;
  }

  set(x, y, dark) {
    this.dark[y][x] = dark;
    this.fixed[y][x] = true;
  }

  /** Finders, timing, alignment, the version blocks and a reserved format area. */
  functions(ver) {
    const n = this.size;
    for (let i = 0; i < n; i++) {
      this.set(6, i, i % 2 === 0);
      this.set(i, 6, i % 2 === 0);
    }
    for (const [x, y] of [[3, 3], [n - 4, 3], [3, n - 4]]) this.finder(x, y);
    const at = alignments(ver, n);
    at.forEach((x, i) => at.forEach((y, j) => {
      const corner = (i === 0 && j === 0) || (i === 0 && j === at.length - 1) || (i === at.length - 1 && j === 0);
      if (!corner) this.square(x, y, 2, (d) => d !== 1);
    }));
    this.format(0);
    if (ver >= 7) this.version(ver);
  }

  finder(cx, cy) {
    this.square(cx, cy, 4, (d) => d !== 2 && d !== 4);
  }

  square(cx, cy, r, darkAt) {
    for (let dy = -r; dy <= r; dy++) {
      for (let dx = -r; dx <= r; dx++) {
        const x = cx + dx;
        const y = cy + dy;
        if (x >= 0 && x < this.size && y >= 0 && y < this.size) this.set(x, y, darkAt(Math.max(Math.abs(dx), Math.abs(dy))));
      }
    }
  }

  format(mask) {
    const data = mask; // level M is 00 in the two high bits
    let rem = data;
    for (let i = 0; i < 10; i++) rem = (rem << 1) ^ ((rem >>> 9) * 0x537);
    const bits = ((data << 10) | rem) ^ 0x5412;
    const bit = (i) => ((bits >>> i) & 1) !== 0;
    const n = this.size;
    for (let i = 0; i <= 5; i++) this.set(8, i, bit(i));
    this.set(8, 7, bit(6));
    this.set(8, 8, bit(7));
    this.set(7, 8, bit(8));
    for (let i = 9; i < 15; i++) this.set(14 - i, 8, bit(i));
    for (let i = 0; i < 8; i++) this.set(n - 1 - i, 8, bit(i));
    for (let i = 8; i < 15; i++) this.set(8, n - 15 + i, bit(i));
    this.set(8, n - 8, true);
  }

  version(ver) {
    let rem = ver;
    for (let i = 0; i < 12; i++) rem = (rem << 1) ^ ((rem >>> 11) * 0x1f25);
    const bits = (ver << 12) | rem;
    for (let i = 0; i < 18; i++) {
      const dark = ((bits >>> i) & 1) !== 0;
      const a = this.size - 11 + (i % 3);
      const b = Math.floor(i / 3);
      this.set(a, b, dark);
      this.set(b, a, dark);
    }
  }

  /** The data, two columns at a time, up and down from the bottom right. */
  codewords(data) {
    const n = this.size;
    let i = 0;
    for (let right = n - 1; right >= 1; right -= 2) {
      if (right === 6) right = 5;
      for (let vert = 0; vert < n; vert++) {
        for (let j = 0; j < 2; j++) {
          const x = right - j;
          const up = ((right + 1) & 2) === 0;
          const y = up ? n - 1 - vert : vert;
          if (this.fixed[y][x] || i >= data.length * 8) continue;
          this.dark[y][x] = ((data[i >>> 3] >>> (7 - (i & 7))) & 1) !== 0;
          i++;
        }
      }
    }
  }

  mask(m) {
    const test = MASKS[m];
    for (let y = 0; y < this.size; y++) {
      for (let x = 0; x < this.size; x++) {
        if (!this.fixed[y][x] && test(x, y)) this.dark[y][x] = !this.dark[y][x];
      }
    }
  }

  /** Runs, blocks and balance. Any mask decodes; this only picks a cleaner one. */
  penalty() {
    const n = this.size;
    const d = this.dark;
    let score = 0;
    let darkCount = 0;
    for (let a = 0; a < n; a++) {
      let rowRun = 1;
      let colRun = 1;
      for (let b = 1; b < n; b++) {
        rowRun = d[a][b] === d[a][b - 1] ? rowRun + 1 : 1;
        colRun = d[b][a] === d[b - 1][a] ? colRun + 1 : 1;
        if (rowRun === 5) score += 3; else if (rowRun > 5) score += 1;
        if (colRun === 5) score += 3; else if (colRun > 5) score += 1;
      }
    }
    for (let y = 0; y < n; y++) {
      for (let x = 0; x < n; x++) {
        if (d[y][x]) darkCount++;
        if (x < n - 1 && y < n - 1 && d[y][x] === d[y][x + 1] && d[y][x] === d[y + 1][x] && d[y][x] === d[y + 1][x + 1]) score += 3;
      }
    }
    return score + Math.floor(Math.abs(darkCount * 20 - n * n * 10) / (n * n)) * 10;
  }
}

const MASKS = [
  (x, y) => (x + y) % 2 === 0,
  (x, y) => y % 2 === 0,
  (x) => x % 3 === 0,
  (x, y) => (x + y) % 3 === 0,
  (x, y) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
  (x, y) => ((x * y) % 2) + ((x * y) % 3) === 0,
  (x, y) => (((x * y) % 2) + ((x * y) % 3)) % 2 === 0,
  (x, y) => (((x + y) % 2) + ((x * y) % 3)) % 2 === 0,
];

function alignments(ver, size) {
  if (ver === 1) return [];
  const count = Math.floor(ver / 7) + 2;
  const step = Math.ceil((ver * 4 + 4) / (count * 2 - 2)) * 2;
  const out = [6];
  for (let pos = size - 7; out.length < count; pos -= step) out.splice(1, 0, pos);
  return out;
}
