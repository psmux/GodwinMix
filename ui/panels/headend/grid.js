// The channel grid, drawn only where it is on screen. Tiles sit absolutely
// in a body as tall as every row would be; scrolling adds the rows coming
// into view and drops those a margin past it. A tile on screen is kept and
// patched in place each second, never rebuilt, so two hundred channels are
// two hundred offsets and a few dozen nodes.

import { Tile } from "./tile.js";

const MARGIN_ROWS = 2;
const GAP = 10;

export class Grid {
  constructor(scroll, body, thumbs) {
    this.scroll = scroll;
    this.body = body;
    this.thumbs = thumbs;
    this.tiles = new Map();
    this.rows = [];
    this.cols = 1;
    this.tileW = 240;
    this.tileH = 200;
  }

  /** Lay out for this width: as many columns as fit at about `min` pixels. */
  layout(min) {
    const w = Math.max(160, this.scroll.clientWidth - 2 * GAP);
    this.cols = Math.max(1, Math.floor((w + GAP) / (min + GAP)));
    this.tileW = (w - GAP * (this.cols - 1)) / this.cols;
    this.tileH = Math.round(this.tileW * 9 / 16) + 78;
  }

  set(rows) {
    this.rows = rows;
    const lines = Math.ceil(rows.length / this.cols);
    this.body.style.height = `${lines * (this.tileH + GAP) + GAP}px`;
  }

  /** The first and last index on screen, without the margin. */
  inView() {
    const top = this.scroll.scrollTop - this.body.offsetTop;
    const line = this.tileH + GAP;
    const first = Math.max(0, Math.floor(top / line));
    const last = Math.floor((top + this.scroll.clientHeight) / line);
    return [first * this.cols, Math.min(this.rows.length - 1, (last + 1) * this.cols - 1)];
  }

  /**
   * Place and patch the tiles in view, drop the rest. Answers the rows on
   * screen. A scroll passes `patch` false: a tile already drawn keeps what it
   * shows until the next second's numbers, and only new ones are drawn.
   */
  draw(seriesOf, selected, patch = true) {
    const [a, b] = this.inView();
    const from = Math.max(0, a - MARGIN_ROWS * this.cols);
    const to = Math.min(this.rows.length - 1, b + MARGIN_ROWS * this.cols);
    const keep = new Set();
    for (let i = from; i <= to; i++) {
      const row = this.rows[i];
      const id = row.show.id;
      keep.add(id);
      let t = this.tiles.get(id);
      const fresh = !t;
      if (fresh) {
        t = new Tile(id, this.thumbs);
        this.tiles.set(id, t);
      }
      const x = GAP + (i % this.cols) * (this.tileW + GAP);
      const y = GAP + Math.floor(i / this.cols) * (this.tileH + GAP);
      const s = t.node.style;
      s.transform = `translate(${x.toFixed(1)}px, ${y}px)`;
      s.width = `${this.tileW.toFixed(1)}px`;
      s.height = `${this.tileH}px`;
      if (t.node.parentNode !== this.body) this.body.append(t.node);
      if (fresh || patch) t.update(row, seriesOf(id), selected === id);
    }
    for (const [id, t] of this.tiles) {
      if (keep.has(id)) continue;
      t.node.remove();
      this.tiles.delete(id);
    }
    const shown = [];
    for (let i = a; i <= b; i++) if (this.rows[i]) shown.push(this.rows[i]);
    return shown;
  }

  /** Bring one channel's tile into view. */
  reveal(id) {
    const i = this.rows.findIndex((r) => r.show.id === id);
    if (i < 0) return;
    const y = Math.floor(i / this.cols) * (this.tileH + GAP) + this.body.offsetTop;
    if (y < this.scroll.scrollTop) this.scroll.scrollTop = y;
    else if (y + this.tileH > this.scroll.scrollTop + this.scroll.clientHeight) this.scroll.scrollTop = y + this.tileH + 2 * GAP - this.scroll.clientHeight;
  }
}
