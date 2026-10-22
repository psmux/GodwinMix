// The headend's small pictures of numbers: a sparkline with a soft area
// under it, a ring gauge with its figure in the middle, and a strip of
// bars for errors per second. Each is built once and then fed numbers, so
// a tile updating every second changes a few attributes and nothing else.

const NS = "http://www.w3.org/2000/svg";
const make = (tag, attrs = {}) => {
  const n = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) n.setAttribute(k, String(v));
  return n;
};

/** A line over the last values, scaled to `max` or to its own peak. */
export function spark(cls, w = 100, h = 24) {
  const s = make("svg", { viewBox: `0 0 ${w} ${h}`, preserveAspectRatio: "none", class: `he-spark ${cls}`, "aria-hidden": "true" });
  const area = make("path", { class: "he-area" });
  const line = make("polyline", { class: "he-line" });
  s.append(area, line);
  return {
    node: s,
    set(values, max) {
      const v = (values || []).filter((x) => x != null);
      if (v.length < 2) {
        line.setAttribute("points", "");
        area.setAttribute("d", "");
        return;
      }
      const top = max || Math.max(...v, 1);
      const step = w / (v.length - 1);
      const pts = v.map((x, i) => `${(i * step).toFixed(1)},${(h - 1 - (Math.min(x, top) / top) * (h - 3)).toFixed(1)}`);
      line.setAttribute("points", pts.join(" "));
      area.setAttribute("d", `M0,${h} L${pts.join(" L")} L${w},${h} Z`);
    },
  };
}

/** A ring that fills to `frac` (0 to 1), with a figure and a word inside. */
export function ring(label, size = 64) {
  const r = size / 2 - 5;
  const len = 2 * Math.PI * r;
  const s = make("svg", { viewBox: `0 0 ${size} ${size}`, class: "he-ring", role: "img" });
  const back = make("circle", { cx: size / 2, cy: size / 2, r, class: "he-ringback" });
  const fill = make("circle", { cx: size / 2, cy: size / 2, r, class: "he-ringfill", "stroke-dasharray": `0 ${len}`, transform: `rotate(-90 ${size / 2} ${size / 2})` });
  const fig = make("text", { x: size / 2, y: size / 2 + 1, class: "he-ringnum", "text-anchor": "middle", "dominant-baseline": "middle" });
  const word = make("text", { x: size / 2, y: size / 2 + 14, class: "he-ringword", "text-anchor": "middle" });
  word.textContent = label;
  s.append(back, fill, fig, word);
  return {
    node: s,
    set(frac, text, level = "") {
      const f = frac == null || Number.isNaN(frac) ? 0 : Math.max(0, Math.min(1, frac));
      fill.setAttribute("stroke-dasharray", `${(f * len).toFixed(1)} ${len.toFixed(1)}`);
      fill.setAttribute("class", `he-ringfill ${level}`);
      fig.textContent = text;
      s.setAttribute("aria-label", `${label} ${text}`);
    },
  };
}

/** Which band a share is in: calm, busy past 70%, hot past 90%. */
export const level = (frac) => (frac == null ? "" : frac >= 0.9 ? "hot" : frac >= 0.7 ? "busy" : "");

/** One thin bar per second, tall where errors were counted. */
export function bars(w = 120, h = 16) {
  const s = make("svg", { viewBox: `0 0 ${w} ${h}`, preserveAspectRatio: "none", class: "he-bars", "aria-hidden": "true" });
  const path = make("path");
  s.append(path);
  return {
    node: s,
    set(values) {
      const v = values || [];
      const step = w / Math.max(v.length, 60);
      const max = Math.max(...v, 1);
      let d = "";
      v.forEach((x, i) => {
        if (!x) return;
        const bh = Math.max(2, (x / max) * h);
        d += `M${(i * step).toFixed(1)},${h} v-${bh.toFixed(1)} h${Math.max(1, step - 0.5).toFixed(1)} v${bh.toFixed(1)} Z`;
      });
      path.setAttribute("d", d);
    },
  };
}
