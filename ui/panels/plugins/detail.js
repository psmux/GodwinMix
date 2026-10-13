// One plugin in full: its pictures, what it does, who makes it, and Install,
// Update and Remove. Opened from a card.

import { el } from "../../shell/dom.js";
import { modal, confirmModal } from "../../shell/modal.js";
import { picture, figures, badges } from "./card.js";
import { tier, kindWords, primary } from "./model.js";
import { perform } from "./act.js";

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

/**
 * @param {() => Promise<object[]>} changed  read the gallery again after a
 *        change, answering its plugins, so the view opens again with the
 *        buttons the plugin now needs
 */
export function openDetail(client, p, changed) {
  const note = el("p.pg-note", { role: "status" });
  const shots = (p.screenshots || []).length ? p.screenshots : [p.thumbnail];
  const body = el("div.pg-detail", {}, [
    el("div.pg-shots", {}, shots.map((url) => picture(p, url, "pg-shot"))),
    el("div.pg-about", {}, [
      el("div.pg-badges", {}, [...badges(p), figures(p)]),
      el("p", { text: p.description || "No description yet." }),
      el("dl.pg-facts", {}, facts(p).flatMap(([k, v]) => [el("dt", { text: k }), el("dd", { text: v })])),
      el("p.row.wrap", {}, [
        p.repository ? el("a", { href: p.repository, target: "_blank", rel: "noopener", text: "Source code" }) : null,
        p.homepage ? el("a", { href: p.homepage, target: "_blank", rel: "noopener", text: "Website" }) : null,
      ]),
      note,
    ]),
  ]);
  const buttons = [];
  const go = async (act, button) => {
    if (act === "remove" && !(await confirmModal(`Remove ${p.title}? Anything using it stops.`, "Remove"))) return;
    for (const b of buttons) b.disabled = true;
    note.classList.remove("bad");
    const done = await perform(client, p, act, (text) => (note.textContent = text));
    note.textContent = done.text;
    note.classList.toggle("bad", !done.ok);
    if (!done.ok) {
      for (const b of buttons) b.disabled = false;
      return button.focus?.();
    }
    const now = (await changed()).find((q) => q.name === p.name);
    if (!now) return;
    m.close();
    openDetail(client, now, changed).say(done.text);
  };
  const first = primary(p);
  if (first.act) buttons.push(el("button.btn.primary", { type: "button", text: first.label, onclick: (e) => go(first.act, e.currentTarget) }));
  if (p.installed) buttons.push(el("button.btn.danger", { type: "button", text: "Remove", onclick: (e) => go("remove", e.currentTarget) }));
  const m = modal({
    title: p.title,
    body,
    wide: true,
    footer: [el("span.grow.pg-dim", { text: first.act ? "" : first.label }), ...buttons, el("button.btn", { type: "button", text: "Close", onclick: () => m.close() })],
  });
  return { ...m, say: (text) => (note.textContent = text) };
}
