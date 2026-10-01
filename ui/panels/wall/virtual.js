// A list that draws only what is on screen, plus a margin. Two hundred
// shows are two hundred offsets and forty nodes; scrolling moves the window
// and the nodes in it are drawn again.

const MARGIN = 6;

export class Virtual {
  /**
   * @param {HTMLElement} scroll the box that scrolls
   * @param {HTMLElement} body the box the rows sit in, inside `scroll`
   * @param {(item: any, index: number) => HTMLElement} render
   */
  constructor(scroll, body, render) {
    this.scroll = scroll;
    this.body = body;
    this.render = render;
    this.items = [];
    this.tops = [0];
    this.shown = [0, -1];
  }

  /** New items, each with its height. Draws at once. */
  set(items, heightOf) {
    this.items = items;
    this.tops = [0];
    for (const it of items) this.tops.push(this.tops.at(-1) + heightOf(it));
    this.body.style.height = `${this.tops.at(-1)}px`;
    this.draw();
  }

  /** The first index whose bottom is below `y`. */
  at(y) {
    let lo = 0;
    let hi = this.items.length;
    while (lo < hi) {
      const mid = (lo + hi) >> 1;
      if (this.tops[mid + 1] <= y) lo = mid + 1;
      else hi = mid;
    }
    return lo;
  }

  /** Indexes in view, margin left out: what is fetched for. */
  inView() {
    const top = this.scroll.scrollTop - this.body.offsetTop;
    const first = this.at(Math.max(0, top));
    const last = Math.min(this.items.length - 1, this.at(top + this.scroll.clientHeight));
    return [first, last];
  }

  draw() {
    const [first, last] = this.inView();
    const from = Math.max(0, first - MARGIN);
    const to = Math.min(this.items.length - 1, last + MARGIN);
    const nodes = [];
    for (let i = from; i <= to; i++) {
      const n = this.render(this.items[i], i);
      n.style.top = `${this.tops[i]}px`;
      n.style.height = `${this.tops[i + 1] - this.tops[i]}px`;
      nodes.push(n);
    }
    this.body.replaceChildren(...nodes);
    this.shown = [from, to];
  }

  /** Scroll as little as it takes to bring one index fully into view. */
  reveal(i) {
    if (i < 0 || i >= this.items.length) return;
    const head = this.body.offsetTop;
    const top = this.tops[i] + head;
    const bottom = this.tops[i + 1] + head;
    const sticky = this.body.previousElementSibling ? this.body.previousElementSibling.offsetHeight : 0;
    if (top - sticky < this.scroll.scrollTop) this.scroll.scrollTop = top - sticky;
    else if (bottom > this.scroll.scrollTop + this.scroll.clientHeight) this.scroll.scrollTop = bottom - this.scroll.clientHeight;
  }
}
