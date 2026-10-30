// Help > Address and token: where this mixer is and the token this page signed
// in with, to copy or to open in a browser already signed in.
//
// The desktop app makes its mixer's token itself on first start, so nobody
// chose it and nobody had seen it; a browser pointed at the mixer then asked
// for a token that was nowhere on screen. The page already holds the token it
// signed in with, so showing it here hands out nothing the page did not have.
// A phone gets its own token from Open on another device instead, one that
// can be taken back. Loaded on first use.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { toast } from "./toast.js";
import { storedToken } from "../client/index.js";
import { signInUrl } from "./devices.js";

const HOW_TO = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/sign-in-from-a-browser.md";

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast({ text: "Copied." });
  } catch {
    toast({ text: "Select the text and copy it; this page may not use the clipboard." });
  }
}

/** One line: a label, the value, and its buttons. */
function line(label, value, buttons) {
  return el("div.col", {}, [el("span.sm.dim", { text: label }), el("div.row", {}, [value, ...buttons])]);
}

/** The token, hidden until asked, with Show and Copy. */
function tokenLine(token) {
  const hidden = "•".repeat(Math.min(token.length, 24));
  const shown = el("code.grow", { text: hidden, style: { wordBreak: "break-all" } });
  const show = el("button.btn.sm", {
    text: "Show",
    onclick: () => {
      const on = shown.textContent === hidden;
      shown.textContent = on ? token : hidden;
      show.textContent = on ? "Hide" : "Show";
    },
  });
  return line("Token", shown, [show, el("button.btn.sm", { text: "Copy", onclick: () => copy(token) })]);
}

/** What the dialog says, for this page's address and token. */
export function signInParts(origin, token, desktop) {
  const base = origin.replace(/\/?$/, "/");
  const link = token ? signInUrl(base, token) : base;
  const parts = [
    el("p", {
      text: token
        ? "Paste the address into a browser on this computer and the token when it asks, or open the link, which signs the browser in by itself."
        : "This mixer asks for no token. Anyone who can reach the address can run it.",
    }),
    line("Address", el("code.grow", { text: base }), [el("button.btn.sm", { text: "Copy", onclick: () => copy(base) })]),
  ];
  if (token) parts.push(tokenLine(token));
  parts.push(
    el("div.row", {}, [
      // The desktop window opens no new tabs; the app opens the system's
      // browser itself, signed in, as GodwinMix > Open in browser does.
      desktop
        ? el("button.btn.sm.primary", { text: "Open in a browser", onclick: () => { location.href = "godwinmix://open-in-browser"; } })
        : el("a.btn.sm.primary", { href: link, target: "_blank", rel: "noopener", text: "Open in a browser" }),
      token ? el("button.btn.sm", { text: "Copy sign in link", onclick: () => copy(link) }) : null,
    ].filter(Boolean)),
  );
  if (desktop) parts.push(el("p.sm.dim", { text: "In the desktop app, GodwinMix > Open in browser does the same. The app keeps this token in a file named core-token in its data folder." }));
  parts.push(
    el("p.sm.dim", { text: "Whoever has this token runs the mixer with the same rights as this page. To let a phone or another computer in, use Help > Open on another device: each device gets its own token, which you can take back." }),
    el("a", { href: HOW_TO, target: "_blank", rel: "noopener", text: "More: signing in from a browser" }),
  );
  return parts;
}

/** The dialog. */
export function openSignIn() {
  const desktop = /GodwinMix-Desktop/.test(navigator.userAgent);
  const body = el("div.col", {}, signInParts(location.origin, storedToken(), desktop));
  return modal({ title: "Address and token", body });
}
