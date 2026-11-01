// Pictures to pick from, for the presenter dialog: a row of tiles that works
// like a set of radio buttons, each with a picture, a swatch or a word.
//
// A group answers `.value` like a select does, so the dialog reads it the
// same way, and the keyboard moves through it with the arrow keys.

import { el, on } from "../../shell/dom.js";
import { previewUrl } from "../graphics/model.js";

/**
 * @param {string} label what the group is for, read out by a screen reader
 * @param {{value: string, label: string, img?: string, swatch?: string, note?: string}[]} options
 * @param {string} chosen the value picked first
 * @param {(value: string) => void} [change]
 */
export function tileGroup(label, options, chosen, change) {
  const group = el("div.gs-tiles", { role: "radiogroup", "aria-label": label });
  let value = chosen;
  const tiles = options.map((o) => tile(o));
  function tile(o) {
    const face = o.img
      ? el("img", { src: o.img, alt: "", loading: "lazy", draggable: "false", onerror: (e) => e.target.replaceWith(el("span.gs-face.gs-none", { text: initials(o.label) })) })
      : el("span.gs-face" + (o.swatch ? `.gs-${o.swatch}` : ".gs-none"), { text: o.swatch ? "" : initials(o.label) });
    const b = el("button.gs-tile", { type: "button", role: "radio", "data-value": o.value, title: o.note || o.label }, [face, el("span.gs-label", { text: o.label }), o.note ? el("span.gs-note", { text: o.note }) : null].filter(Boolean));
    on(b, "click", () => pick(o.value));
    on(b, "keydown", (e) => {
      const step = { ArrowRight: 1, ArrowDown: 1, ArrowLeft: -1, ArrowUp: -1 }[e.key];
      if (!step) return;
      e.preventDefault();
      const i = options.findIndex((x) => x.value === value);
      const next = options[(i + step + options.length) % options.length];
      pick(next.value);
      tiles[options.indexOf(next)].focus();
    });
    return b;
  }
  function pick(v) {
    value = v;
    for (const t of tiles) {
      const picked = t.dataset.value === v;
      t.classList.toggle("on", picked);
      t.setAttribute("aria-checked", picked ? "true" : "false");
      t.tabIndex = picked ? 0 : -1;
    }
    if (change) change(v);
  }
  for (const t of tiles) group.appendChild(t);
  Object.defineProperty(group, "value", { get: () => value, set: (v) => pick(v) });
  group.add = (o, first) => {
    const t = tile(o);
    tiles.push(t);
    options.push(o);
    if (first) group.insertBefore(t, group.firstChild);
    else group.appendChild(t);
    return t;
  };
  pick(chosen);
  return group;
}

/** Sources that could show a person: not a picture, a page, a text or a
 * test pattern. Every source when none looks like one. */
export function people(sources, first) {
  const still = /image|html|template|text|lyrics|slides|browser|colou?r|test|ticker|clock/i;
  const live = (sources || []).filter((s) => !still.test(`${s.type || ""} ${s.uri || ""}`));
  const list = live.length ? live : sources || [];
  return [...list].sort((a, b) => (b.id === first) - (a.id === first));
}

/** Two letters for a tile with no picture: "Studio camera" is "SC". */
export function initials(text) {
  const words = String(text || "?").replace(/\.[a-z0-9]+$/i, "").split(/[\s._-]+/).filter(Boolean);
  return (words.length > 1 ? words[0][0] + words[1][0] : (words[0] || "?").slice(0, 2)).toUpperCase();
}

/** A numbered step heading. */
export function step(n, text, hint) {
  return el("div.gs-step", {}, [el("span.gs-num", { text: String(n) }), el("span.gs-title", { text }), hint ? el("span.sm.faint", { text: hint }) : null].filter(Boolean));
}

/** The backgrounds to show as pictures: the gallery's studio sets (a plate,
 * a desk and where the presenter stands), its full frame pictures, clips and
 * pages, then library files, which have no picture of their own. A set's
 * value is `set:<id>`, since a set is placed rather than drawn as one source. */
export function backgrounds(client, gallery, media) {
  const sets = (gallery || []).filter((i) => i.kind === "set");
  const full = (gallery || []).filter((i) => i.zone === "full" && i.uri && !["set", "transition", "effect"].includes(i.kind));
  return [
    ...sets.map((i) => ({ value: `set:${i.id}`, label: i.name, img: previewUrl(client, i, 240), note: "Studio set" })),
    ...full.map((i) => ({ value: i.uri, label: i.name, img: previewUrl(client, i, 240) })),
    ...(media || []).map((m) => ({ value: m.name, label: m.name })),
  ];
}
