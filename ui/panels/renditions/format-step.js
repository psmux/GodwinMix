// The Format step of Add destination: what the platform receives.
//
// "Same as the source" is always the first card and always free. Then the
// presets this machine can make, each with its size, rate, bitrate and a
// badge for what it costs; the ones it cannot make stay on show, dimmed,
// with the reason, so nobody wonders where 1080p60 went. Custom opens a form
// under the cards. The platform picks the starting card.

import { el } from "../../shell/dom.js";
import { COPY, COST_WORDS, costClass, describe, initialChoice, renditionFor, singles, usable, sourceMatches } from "./model.js";
import { loadPresets, roomNow, stylesheet } from "./shared.js";
export { programmeShape, channelShape } from "./shared.js";
import { customForm } from "./custom.js";

/**
 * @param {object} client
 * @param {{platform?: string, platformTitle?: string, shape?: object|null,
 *          current?: object, id?: () => string}} opts
 * @returns {{node: HTMLElement, ready: Promise<void>, value: () => object|undefined|null, changed: () => boolean}}
 */
export function formatStep(client, opts = {}) {
  stylesheet();
  const cards = el("div.rnd-cards", { role: "radiogroup", "aria-label": "Format" });
  const extra = el("div.rnd-customwrap", { hidden: true });
  const lede = el("span.rnd-dim", { text: "Reading what this machine can make" });
  const node = el("section.rnd-step", {}, [el("div.rnd-steph", {}, [el("span.rnd-kicker", { text: "Format" }), lede]), cards, extra]);
  const state = { choice: undefined, first: undefined, custom: null, presets: null };

  const pick = async (id) => {
    state.choice = id;
    for (const c of cards.children) c.setAttribute("aria-checked", String(c.dataset.id === id));
    extra.hidden = id !== "custom";
    if (id === "custom" && !state.custom) {
      state.custom = await customForm({ request: opts.current && !opts.current.preset ? opts.current : null, codecs: codecsOf(state.presets) });
      extra.appendChild(state.custom.el);
    }
  };

  const ready = (async () => {
    const [presets, room, shape] = await Promise.all([loadPresets(client), roomNow(client), Promise.resolve(opts.shape)]);
    state.presets = presets;
    if (!presets) {
      // A core with no renditions: the output is what it always was.
      node.hidden = true;
      return;
    }
    const offered = singles(presets);
    const start = initialChoice(presets, opts.platform, shape, opts.current);
    const suggested = initialChoice(presets, opts.platform, null);
    cards.append(copyCard(shape, offered.find((p) => p.id === suggested), pick));
    for (const p of offered) cards.appendChild(presetCard(p, room, p.id === suggested && opts.platformTitle, pick));
    cards.appendChild(customCard(pick));
    lede.textContent = opts.platformTitle ? `What ${opts.platformTitle} receives` : "What is sent";
    state.first = start;
    await pick(start);
  })();

  const changed = () => state.choice !== state.first || state.choice === "custom";
  return {
    node,
    ready,
    changed,
    /**
     * The `rendition` to send. Undefined sends nothing: a copy on an add, or
     * an edit that left the format alone. Null clears the one an edit had.
     */
    value() {
      if (!state.presets || (opts.current && !changed())) return undefined;
      const id = (opts.id && opts.id()) || "custom";
      const r = renditionFor(state.choice, state.custom && state.custom.request(id));
      return r === undefined && opts.current ? null : r;
    },
  };
}

function card(id, title, meta, badge, note, pick, disabled) {
  const b = el("button.rnd-card", { type: "button", role: "radio", "aria-checked": "false", disabled: !!disabled, onclick: () => pick(id) }, [
    el("span.rnd-ctop", {}, [el("span.rnd-ctitle", { text: title }), badge ? el(`span.rnd-badge.${badge}`, { text: COST_WORDS[badge] }) : null]),
    el("span.rnd-meta", { text: meta }),
    note ? el("span.rnd-note", { text: note }) : null,
  ]);
  b.dataset.id = id;
  return b;
}

function copyCard(shape, suggestion, pick) {
  const matches = suggestion && sourceMatches(shape, suggestion.request);
  const note = matches ? `Already matches ${suggestion.title}` : "";
  return card(COPY, "Same as the source", "No re-encoding", "free", note, pick);
}

function presetCard(p, room, suggestedFor, pick) {
  const d = describe(p.request);
  // Each piece kept whole, so a narrow card breaks between them, never inside "6 Mb/s".
  const meta = [d.size, d.fps, d.bitrate, d.codec].filter(Boolean).map((x) => x.replace(/ /g, "\u00a0")).join(" · ");
  const note = !usable(p) ? p.why || "This machine cannot make it" : suggestedFor ? `Suggested for ${suggestedFor}` : "";
  const c = card(p.id, p.title, meta, usable(p) ? costClass(p, room) : null, note, pick, !usable(p));
  if (!usable(p)) c.classList.add("off");
  return c;
}

function customCard(pick) {
  return card("custom", "Custom", "Codec, size, rate and bitrate", null, "", pick);
}

/** The video codecs some available preset uses, which the machine can therefore encode. */
function codecsOf(presets) {
  const out = new Set();
  for (const p of presets || []) if (usable(p) && p.request && p.request.video && p.request.video.codec) out.add(p.request.video.codec);
  return out;
}
