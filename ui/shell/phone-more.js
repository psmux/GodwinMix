// The More screen: every panel without a tab of its own, then the handful of
// menu items a phone operator reaches for mid show. Everything else is still
// in the menu button at the top, which carries the whole menu bar.

import { el } from "./dom.js";
import { run } from "./commands.js";
import { settings } from "./settings.js";
import { icon } from "./phone-icons.js";
import { installWay, promptInstall, onInstallChange, noteAuthority } from "./install.js";
import { TRUST_HELP } from "./trust.js";

/** What a panel is for, in a line, for the panels the core ships. */
const ABOUT = {
  "core/graphics": ["graphics", "Lower thirds, tickers, bugs and moving sets"],
  "core/media": ["media", "Clips, pictures and sounds to play out"],
  "core/alerts": ["alerts", "What the mixer has to tell you"],
  "core/channels": ["channels", "Streams to a platform, by channel"],
};

/** The deck showing More, so the Install app row comes and goes with the offer. */
let shown = null;
onInstallChange(() => {
  if (shown && shown.page === "more" && shown.more.isConnected) shown.render();
});

/** Ask once whether the mixer has an authority a phone could trust. */
let asked = false;
function askAuthority(client) {
  if (asked || !client || typeof client.call !== "function") return;
  asked = true;
  client.call("core.info", {}).then((info) => noteAuthority(!!(info && info.tls && info.tls.authority)), () => { asked = false; });
}

/** The screen's children, built again each time it is shown. */
export function morePage(deck, extras) {
  shown = deck;
  askAuthority(deck.client);
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
      installRow(installWay(), () => promptInstall()),
    ]),
  ].filter(Boolean);
}

/**
 * Install app, in the form this browser allows: a button where the browser
 * has offered to install, a line saying where Add to Home Screen is on an
 * iPhone or iPad, a link to how to trust the mixer where that comes first,
 * and nothing at all where none applies or it is done.
 */
export function installRow(way, onclick) {
  if (way === "prompt") return row("install", "Install app", "Open the mixer from the home screen, full screen", onclick);
  if (way === "trust") {
    return el("a.phone-row", { href: TRUST_HELP, target: "_blank", rel: "noopener" }, [
      icon("install"),
      el("span.phone-row-text", {}, [el("strong", { text: "Install app" }), el("span", { text: "Trust this mixer on your phone first: how" })]),
      el("span.phone-chevron", { "aria-hidden": "true", text: "›" }),
    ]);
  }
  if (way !== "ios") return null;
  return el("div.phone-row.phone-row-note", {}, [
    icon("install"),
    el("span.phone-row-text", {}, [el("strong", { text: "Install app" }), el("span", { text: "Tap Share, then Add to Home Screen" })]),
  ]);
}

function row(art, title, line, onclick, on) {
  return el("button.phone-row", { type: "button", onclick, "aria-pressed": on === undefined ? null : String(on) }, [
    icon(art),
    el("span.phone-row-text", {}, [el("strong", { text: title }), el("span", { text: line })]),
    on === undefined ? el("span.phone-chevron", { "aria-hidden": "true", text: "›" }) : el("span.phone-switch", { "aria-hidden": "true" }),
  ]);
}
