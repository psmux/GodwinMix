// Drawing the two still layers: the links under the dots, the cards over
// them. Done once per snapshot, hover or move of the view, never per frame.
// Only what is on screen is drawn, and a card drawn small loses its words
// before it loses its colour, so two hundred channels zoomed out still read.

const R = 7;

/** The colour of a tone, from the theme's tokens. */
export const toneColour = (col, tone) => ({ ok: col.ok, busy: col.accent, warn: col.warn, bad: col.bad, off: col.faint }[tone] || col.faint);

function fit(g, s, max) {
  if (!s || g.measureText(s).width <= max) return s || "";
  let lo = 0, hi = s.length;
  while (lo < hi) {
    const m = (lo + hi + 1) >> 1;
    if (g.measureText(s.slice(0, m) + "…").width <= max) lo = m;
    else hi = m - 1;
  }
  return s.slice(0, lo) + "…";
}

const onScreen = (b, v) => b[2] * v.s + v.tx > -40 && b[0] * v.s + v.tx < v.w + 40 && b[3] * v.s + v.ty > -40 && b[1] * v.s + v.ty < v.h + 40;

export function paintLinks(g, links, v, col, look) {
  g.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  g.clearRect(0, 0, v.w, v.h);
  g.translate(v.tx, v.ty);
  g.scale(v.s, v.s);
  for (const l of links) {
    if (!l.pts || !onScreen(l.box, v)) continue;
    const lit = look.lit && look.lit.has(l.id);
    if (l.uses && !lit) continue;
    const dim = look.dims(l.a) || look.dims(l.b) || (look.lit && !lit);
    const dead = !l.kbps && !l.uses;
    g.beginPath();
    g.moveTo(l.c[0].x, l.c[0].y);
    g.bezierCurveTo(l.c[1].x, l.c[1].y, l.c[2].x, l.c[2].y, l.c[3].x, l.c[3].y);
    g.setLineDash(dead || l.uses ? [5, 5] : []);
    g.lineWidth = (lit ? 2.6 : l.straight ? 1.4 : 1.8) / Math.max(0.6, v.s);
    g.strokeStyle = l.tone === "warn" || l.tone === "bad" ? toneColour(col, l.tone) : lit ? col.accent : dead ? col.faint : col.link;
    g.globalAlpha = (dim ? 0.18 : 1) * (dead ? 0.6 : lit ? 1 : 0.85);
    g.stroke();
  }
  g.setLineDash([]);
  g.globalAlpha = 1;
}

export function paintCards(g, nodes, v, col, look, font) {
  g.setTransform(v.dpr, 0, 0, v.dpr, 0, 0);
  g.clearRect(0, 0, v.w, v.h);
  g.translate(v.tx, v.ty);
  g.scale(v.s, v.s);
  for (const n of nodes) {
    if (n.hidden || !onScreen([n.x - n.w / 2, n.y - n.h / 2, n.x + n.w / 2, n.y + n.h / 2], v)) continue;
    g.globalAlpha = look.dims(n.id) || (look.lit && !look.lit.has(n.id)) ? 0.28 : 1;
    card(g, n, v.s, col, font, look.hover === n.id || look.selected === n.id);
  }
  g.globalAlpha = 1;
}

function card(g, n, s, col, font, hot) {
  const x = n.x - n.w / 2, y = n.y - n.h / 2;
  const tc = toneColour(col, n.tone);
  g.beginPath();
  g.roundRect(x, y, n.w, n.h, R);
  g.fillStyle = n.kind === "gpu" || n.kind === "program" ? col.panel2 : col.panel;
  g.fill();
  g.lineWidth = (hot ? 2 : n.tone === "ok" ? 1 : 1.5) / s;
  g.strokeStyle = hot ? col.accent : n.tone === "ok" || n.tone === "off" ? col.line : tc;
  g.setLineDash(n.tone === "off" ? [4, 3] : []);
  g.stroke();
  g.setLineDash([]);
  g.fillStyle = n.tally === "program" ? col.live : tc;
  g.beginPath();
  g.roundRect(x + 2, y + 5, 3, n.h - 10, 1.5);
  g.fill();
  if (n.h * s < 13) return;
  const pad = 11, big = n.h >= 40;
  const two = n.h * s >= 24;
  let ty = big || n.h > 36 ? y + 16 : two ? y + 12 : y + n.h / 2 + 4;
  const right = n.num || n.badge || (n.kind === "proc" ? n.lines[0] : "") || (n.tally === "program" ? "ON AIR" : n.tally === "preview" ? "PVW" : "");
  g.textBaseline = "alphabetic";
  g.textAlign = "right";
  g.font = font(600, 10.5, n.num ? "num" : "");
  g.fillStyle = n.tally === "program" ? col.live : n.badge || n.kind === "proc" ? (n.gpu || n.badge ? col.ok : col.dim) : col.dim;
  const rw = right ? g.measureText(right).width + 8 : 0;
  if (right) g.fillText(right, x + n.w - pad + 2, ty);
  g.textAlign = "left";
  g.font = font(600, 12.5);
  g.fillStyle = n.tone === "off" ? col.dim : col.text;
  g.fillText(fit(g, n.title, n.w - pad * 2 - rw), x + pad, ty);
  if (!two && !big) return;
  g.font = font(400, 10.5);
  const lines = n.kind === "proc" ? n.lines.slice(1) : n.lines;
  const room = Math.max(1, Math.floor((n.h - 18) / 13));
  if (!big && n.h < 36) {
    ty += 12;
    g.fillStyle = n.tone === "warn" || n.tone === "bad" ? tc : col.dim;
    g.fillText(fit(g, lines.filter(Boolean).join(" · "), n.w - pad * 2), x + pad, ty);
    return;
  }
  for (const line of lines.slice(0, room)) {
    ty += 13;
    if (!line) continue;
    g.fillStyle = /error|Nothing|Failed|Black/.test(line) ? tc : col.dim;
    g.fillText(fit(g, line, n.w - pad * 2), x + pad, ty);
  }
}

/** Column headings, above each column, in the map's units. */
export function paintHeads(g, cols, v, col, font, words) {
  g.font = font(600, 10.5);
  g.fillStyle = col.faint;
  g.textAlign = "center";
  cols.x.forEach((x, i) => { if (cols.w[i] && words[i]) g.fillText(words[i].toUpperCase(), x, cols.top - 22); });
  g.textAlign = "left";
}
