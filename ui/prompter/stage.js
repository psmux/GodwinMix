// The text and the cue line. Every row is one line high and never wraps, so
// row n is at n line heights from the top and drawing the position is one
// transform: no measuring while it scrolls, and nothing for the browser to
// lay out again.

import { drawRow, plain } from "./text.js";

const FONT = 'system-ui, -apple-system, "Segoe UI", Roboto, Helvetica, Arial, sans-serif';
const LEADING = 1.3;

export class Stage {
  constructor(root) {
    this.root = root;
    this.frame = document.createElement("div");
    this.frame.className = "prompter-frame";
    this.track = document.createElement("div");
    this.track.className = "prompter-track";
    this.cueLine = document.createElement("div");
    this.cueLine.className = "prompter-cue";
    this.empty = document.createElement("p");
    this.empty.className = "prompter-empty";
    this.frame.append(this.track);
    root.append(this.frame, this.cueLine, this.empty);
    this.rows = [];
    this.look = { line_chars: 32, margin: 8, cue: 0.33, mirror: false };
    this.mirrored = false;
    this.lh = 40;
    this.cueY = 0;
    this.widest = 0;
  }

  /** Say something in place of a script: none loaded, or no connection. */
  say(words) {
    this.empty.textContent = words || "";
    this.empty.hidden = !words;
  }

  setRows(rows) {
    this.rows = rows || [];
    const frag = document.createDocumentFragment();
    let on = { bold: false, italic: false };
    for (const row of this.rows) {
      const drawn = drawRow(row, on);
      on = drawn.on;
      frag.appendChild(drawn.node);
    }
    this.track.replaceChildren(frag);
    this.widest = widestAt100(this.rows);
    this.layout();
  }

  setLook(look, mirrored) {
    this.look = { ...this.look, ...look };
    this.mirrored = !!mirrored;
    this.layout();
  }

  /** Fit `line_chars` characters across the width left between the margins. */
  layout() {
    const width = this.root.clientWidth || window.innerWidth;
    const height = this.root.clientHeight || window.innerHeight;
    const side = (width * this.look.margin) / 100;
    const room = Math.max(width - 2 * side, 100);
    const per100 = Math.max(this.widest, this.look.line_chars * averageAt100());
    const size = Math.max(8, (room * 100) / per100);
    this.lh = size * LEADING;
    this.cueY = height * this.look.cue;
    Object.assign(this.track.style, {
      fontSize: size + "px",
      lineHeight: this.lh + "px",
      left: side + "px",
      right: side + "px",
    });
    this.track.style.setProperty("--lh", this.lh + "px");
    this.cueLine.style.top = this.cueY + "px";
    this.cueLine.style.height = this.lh + "px";
    this.cueLine.style.marginTop = -this.lh / 2 + "px";
    this.frame.classList.toggle("mirrored", this.mirrored);
  }

  /**
   * Put the point `position` rows down the script on the cue line: a whole
   * number fills the band with that row, and a half is half way through it.
   */
  draw(position) {
    const y = this.cueY - this.lh / 2 - position * this.lh;
    this.track.style.transform = `translate3d(0, ${y.toFixed(2)}px, 0)`;
  }
}

let ctx = null;
function measure(text) {
  if (!ctx) {
    ctx = document.createElement("canvas").getContext("2d");
    ctx.font = `bold 100px ${FONT}`;
  }
  return ctx.measureText(text).width;
}

let average = 0;
function averageAt100() {
  if (!average) {
    const sample = "the quick brown fox jumps over the lazy dog, Then Sphinx of black quartz";
    average = measure(sample) / sample.length;
  }
  return average;
}

/** The widest row in bold at 100 px, so no row is ever cut at the edge. */
function widestAt100(rows) {
  let widest = 0;
  for (const row of rows) widest = Math.max(widest, measure(plain(row.text)));
  return widest;
}
