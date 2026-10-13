// One plugin in full: a large carousel of its pictures and clips, what it
// does, who makes it, and Install, Open, Update and Remove. Opened from a
// card.

import { el } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { picture, figures, badges } from "./card.js";
import { tier, kindWords } from "./model.js";
import { carousel } from "./media.js";

let current = null;

/** "12 March 2026" from an RFC 3339 time, or "" for none. */
export function when(stamp) {
  const t = Date.parse(stamp || "");
  return Number.isNaN(t) ? "" : new Date(t).toLocaleDateString(undefined, { day: "numeric", month: "long", year: "numeric" });
}

/** The rows of facts, leaving out what the listing does not say. */
export function facts(p) {
  const rows = [
    ["What it adds", kindWords(p).join(", ")],
    ["Version", p.installed_version && p.installed_version !== p.version ? `${p.version} (you have ${p.installed_version})` : p.version],
    ["Made by", p.author],
    ["Licence", p.license],
    ["Last changed", when(p.updated)],
    ["Quality", `${tier(p).label}. ${tier(p).why}`],
    ["On this computer", p.fit_detail],
    ["Signed", p.bundled ? "" : p.signed ? "Yes" : "No. Nobody has signed this download."],
    ["Older versions", (p.versions || []).length > 1 ? p.versions.slice(0, -1).reverse().join(", ") : ""],
  ];
  return rows.filter(([, v]) => v);
}

/** Close the detail view, when one is open. */
export function closeDetail() {
  current?.close();
  current = null;
}

/**
 * @param {() => Promise<object[]>} changed  read the gallery again after a
 *        change, answering its plugins, so the view opens again with the
 *        buttons the plugin now needs
 * @param {object} store  the page's buttons and what they do (store.js)
 */
export function openDetail(client, p, changed, store) {
  closeDetail();
  const note = el("div.pg-note", { role: "status" });
  const shown = store.note(p.name);
  const hero = carousel(p);
  const head = el("div.pg-head", {}, [
    p.icon ? picture(p, p.icon, "pg-appicon") : null,
    el("div", {}, [el("strong", { text: p.title }), el("span.pg-dim", { text: p.author || "" })]),
    el("span.grow"),
  ]);
  const body = el("div.pg-detail", { "data-name": p.name }, [
    hero,
    el("div.pg-about", {}, [
      head,
      el("div.pg-badges", {}, [...badges(p), figures(p)]),
      el("p", { text: p.description || "No description yet." }),
      el("dl.pg-facts", {}, facts(p).flatMap(([k, v]) => [el("dt", { text: k }), el("dd", { text: v })])),
      el("p.row.wrap", {}, [
        p.repository ? el("a", { href: p.repository, target: "_blank", rel: "noopener", text: "Source code" }) : null,
        p.homepage ? el("a", { href: p.homepage, target: "_blank", rel: "noopener", text: "Website" }) : null,
      ]),
      shown || note,
    ]),
  ]);
  // After a change the view opens again, with the buttons the plugin now needs.
  const again = async (done, act) => {
    if (!done || act === "open" || current !== m) return;
    const now = (await changed()).find((q) => q.name === p.name) || { ...p, installed: false, update: false };
    if (current === m) openDetail(client, now, changed, store);
  };
  const go = store.button(p, ".pg-big", again);
  head.append(go);
  const remove = async () => {
    if (!(await confirmModal(`Remove ${p.title}? Anything using it stops.`, "Remove"))) return;
    again(await store.press(p, "remove"), "remove");
  };
  const m = modal({
    title: p.title,
    body,
    wide: true,
    onClose: () => {
      hero.stop();
      if (current === m) current = null;
    },
    footer: [
      el("span.grow.pg-dim", { text: go.dataset.act ? "" : go.textContent }),
      p.installed ? el("button.btn.danger", { type: "button", text: "Remove", onclick: remove }) : null,
      el("button.btn", { type: "button", text: "Close", onclick: () => m.close() }),
    ],
  });
  current = m;
  return { ...m, say: (text) => (note.textContent = text) };
}
