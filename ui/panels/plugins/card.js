// One plugin as a card: its picture, its name, one line about it, stars and
// downloads, and the one button that matters. The picture is loaded by the
// browser from GitHub; one that fails turns into the kind's icon.

import { el, svg } from "../../shell/dom.js";
import { ICONS, KIND_COLOUR, kindOfSource } from "../../client/kind-look.js";
import { numbers, tier, oneLine, kindWords, primary } from "./model.js";

export const GLYPHS = {
  star: "M12 3.5l2.6 5.3 5.9.9-4.3 4.1 1 5.8L12 16.9l-5.2 2.7 1-5.8-4.3-4.1 5.9-.9z",
  down: "M12 4v11m-5-5l5 5 5-5M5 20h14",
};

/** The icon a plugin with no picture is drawn with. */
function look(p) {
  const named = kindOfSource({ type: p.name, uri: "" });
  if (named !== "file") return named;
  const kinds = p.kinds || [];
  if (kinds.includes("output")) return "output";
  if (kinds.includes("graphic")) return "graphic";
  if (kinds.includes("transition")) return "media";
  if (kinds.includes("source")) return "stream";
  return "exec";
}

/** A plugin's picture, or its kind's icon on a tinted ground. */
export function picture(p, url, cls = "pg-pic") {
  const icon = look(p);
  const stand = () => {
    const node = el(`div.${cls}.pg-stand`, {}, [svg(ICONS[icon] || ICONS.more, cls === "pg-icon" ? 16 : 40)]);
    node.style.setProperty("--tone", KIND_COLOUR[icon] || "var(--accent)");
    return node;
  };
  if (!url) return stand();
  const img = el(`img.${cls}`, { src: url, alt: "", loading: "lazy", referrerpolicy: "no-referrer" });
  img.addEventListener("error", () => img.replaceWith(stand()), { once: true });
  return img;
}

/** The stars and downloads, each with its glyph. */
export function figures(p) {
  return el("span.pg-nums", {}, numbers(p).map((n) => el("span", { title: n.title }, [svg(GLYPHS[n.icon], 14), n.text])));
}

/** The small badges: included, installed, tier. */
export function badges(p) {
  const out = [];
  if (p.bundled) out.push(el("span.pg-badge.inc", { text: "Included", title: "Comes with GodwinMix. Installs with no download." }));
  if (p.installed) out.push(el("span.pg-badge.ok", { text: "Installed" }));
  const t = tier(p);
  out.push(el(`span.pg-badge.tier.${p.tier || "custom"}`, { text: t.label, title: t.why }));
  return out;
}

/**
 * @param {object} p        one entry of `plugin.gallery`
 * @param {(p) => void} open the detail view
 * @param {(p, act, button) => void} act Install or Update
 */
export function card(p, open, act) {
  const go = primary(p);
  const button = el(`button.btn${go.act ? ".primary" : ""}`, {
    type: "button",
    text: go.label,
    disabled: !go.act,
    title: go.act ? null : p.fit_detail || null,
    onclick: (e) => { e.stopPropagation(); act(p, go.act, button); },
  });
  const fits = el(`span.pg-fit${p.runs_here ? ".yes" : ".no"}`, { text: p.fit, title: p.fit_detail });
  return el("article.pg-card", { "data-name": p.name }, [
    el("button.pg-open", { type: "button", "aria-label": `${p.title}: more about it`, onclick: () => open(p) }, [
      picture(p, p.thumbnail),
      el("div.pg-text", {}, [
        el("div.pg-title", {}, [p.icon ? picture(p, p.icon, "pg-icon") : null, el("strong", { text: p.title })]),
        el("p.pg-line", { text: oneLine(p.description) || "No description yet." }),
        el("div.pg-meta", {}, [figures(p), el("span.pg-kinds", { text: kindWords(p).join(", ") })]),
      ]),
    ]),
    el("div.pg-foot", {}, [el("span.pg-badges", {}, badges(p)), fits, el("span.grow"), button]),
  ]);
}
