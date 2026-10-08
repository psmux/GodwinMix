// The More screen: every panel without a tab of its own, then the handful of
// menu items a phone operator reaches for mid show. Everything else is still
// in the menu button at the top, which carries the whole menu bar.

import { el } from "./dom.js";
import { run } from "./commands.js";
import { settings } from "./settings.js";
import { icon } from "./phone-icons.js";

/** What a panel is for, in a line, for the panels the core ships. */
const ABOUT = {
  "core/graphics": ["graphics", "Lower thirds, tickers, bugs and moving sets"],
  "core/media": ["media", "Clips, pictures and sounds to play out"],
  "core/alerts": ["alerts", "What the mixer has to tell you"],
  "core/channels": ["channels", "Streams to a platform, by channel"],
};

/** The screen's children, built again each time it is shown. */
export function morePage(deck, extras) {
  const studio = !!settings().producer;
  const cards = extras.map((p) => {
    const [art, line] = ABOUT[p.id] || ["plugin", p.plugin ? "From the " + p.plugin + " plugin" : "A panel"];
    return el("button.phone-card", { type: "button", onclick: () => deck.go("panel:" + p.id) }, [
      icon(art), el("strong", { text: p.title }), el("span", { text: line }),
    ]);
  });
  return [
    cards.length ? el("h2.phone-group", { text: "Panels" }) : null,
    cards.length ? el("div.phone-cards", {}, cards) : null,
    el("h2.phone-group", { text: "On air" }),
    el("div.phone-list", {}, [
      row("studio", "Studio mode", studio ? "On: preview, then Take" : "Off: a tap goes straight to air", () => run("view.studio").then(() => deck.render()), studio),
      row("black", "Cut to black", "Take the programme to black now", () => run("program.black")),
      row("routing", "Routing", "Which input goes to which output", () => run("view.routing")),
      row("wall", "Monitoring wall", "Every show and feed at a glance", () => run("view.wall")),
    ]),
    el("h2.phone-group", { text: "This mixer" }),
    el("div.phone-list", {}, [
      row("settings", "Page settings", "Theme, tiles and how this page behaves", () => run("shell.settings")),
      row("mixer", "Mixer settings", "Canvas, encoder, token and safety", () => run("mixer.settings")),
      row("device", "Open on another device", "A code or a link for a second phone", () => run("help.devices")),
    ]),
  ];
}

function row(art, title, line, onclick, on) {
  return el("button.phone-row", { type: "button", onclick, "aria-pressed": on === undefined ? null : String(on) }, [
    icon(art),
    el("span.phone-row-text", {}, [el("strong", { text: title }), el("span", { text: line })]),
    on === undefined ? el("span.phone-chevron", { "aria-hidden": "true", text: "›" }) : el("span.phone-switch", { "aria-hidden": "true" }),
  ]);
}
