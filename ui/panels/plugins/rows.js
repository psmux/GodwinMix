// The two rows above the grid, the way an app store opens: Featured, large,
// and New and updated, by date. Shown on All with no search, since a search
// or a chip means a person already knows what they want.

import { el } from "../../shell/dom.js";
import { picture } from "./card.js";
import { oneLine, kindWords } from "./model.js";

/**
 * The Featured row: what the index marked featured, even one, else the
 * plugins that run here and have a picture, best first, when there are at
 * least two of them.
 */
export function featured(plugins, max = 6) {
  const marked = plugins.filter((p) => p.featured);
  if (marked.length) return marked.slice(0, max);
  const pictured = plugins.filter((p) => p.runs_here && (p.thumbnail || (p.media || []).length)).slice(0, max);
  return pictured.length >= 2 ? pictured : [];
}

/** New and updated: the most recently changed, newest first. */
export function fresh(plugins, max = 8) {
  const when = (p) => Date.parse(p.updated || "") || 0;
  const out = plugins.filter((p) => when(p) > 0).sort((a, b) => when(b) - when(a)).slice(0, max);
  return out.length >= 2 ? out : [];
}

/**
 * One large Featured card: the picture, the name and the line over it, and
 * the same button the grid card has.
 *
 * @param {(p) => void} open           the detail view
 * @param {(p) => HTMLElement} button  the card's primary button
 */
export function featureCard(p, open, button) {
  const art = p.thumbnail || ((p.media || []).find((m) => m.type === "image") || {}).url || "";
  return el("article.pg-feature", { "data-name": p.name }, [
    el("button.pg-open", { type: "button", "aria-label": `${p.title}: more about it`, onclick: () => open(p) }, [
      picture(p, art, "pg-pic"),
      el("div.pg-over", {}, [
        el("span.pg-kicker", { text: kindWords(p).join(", ") }),
        el("strong", { text: p.title }),
        el("span", { text: oneLine(p.description) }),
      ]),
    ]),
    el("div.pg-feature-go", {}, [button(p)]),
  ]);
}

/**
 * A titled row that scrolls sideways, with snap points so a swipe lands on
 * a card.
 */
export function row(title, nodes) {
  return el("section.pg-row", { "aria-label": title }, [
    el("h3", { text: title }),
    el("div.pg-strip", {}, nodes),
  ]);
}
