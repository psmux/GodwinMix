// Help > Address and sign in: where this mixer is and how this page signed
// in. On a mixer with accounts that is the person's own account, with Sign
// out and Change password, and the token sits under Advanced for agents and
// Companion. On one without, it is the token this page signed in with, to
// copy or to open in a browser already signed in.
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

/**
 * The same for a mixer with accounts: the address, who this page is, and the
 * token only under Advanced. `actions` holds signOut and changePassword.
 */
export function accountParts(origin, token, account, actions) {
  const base = origin.replace(/\/?$/, "/");
  const parts = [
    el("p", { text: "Open the address in a browser on any device that reaches this mixer and sign in with a username and password. An admin adds people in Mixer settings > Users." }),
    line("Address", el("code.grow", { text: base }), [el("button.btn.sm", { text: "Copy", onclick: () => copy(base) })]),
  ];
  if (account.via === "session") {
    parts.push(
      el("p", { text: `Signed in as ${account.user}, ${ROLE_WORDS[account.role] || account.role}.` }),
      el("div.row", {}, [
        el("button.btn.sm", { text: "Change password", onclick: () => actions.changePassword() }),
        el("button.btn.sm", { text: "Sign out", onclick: () => actions.signOut() }),
      ]),
    );
  } else {
    parts.push(el("p.sm.dim", { text: "This page is signed in with a token rather than an account." }));
  }
  if (token) {
    parts.push(el("details.sm", {}, [
      el("summary.dim", { text: "Advanced: the token, for agents and Companion" }),
      el("p.sm.dim", { text: "Agents, Bitfocus Companion and scripts still use a token. Whoever has it runs the mixer with the same rights as this page." }),
      tokenLine(token),
    ]));
  }
  parts.push(el("a", { href: HOW_TO_ACCOUNTS, target: "_blank", rel: "noopener", text: "More: signing in with a username and password" }));
  return parts;
}

const ROLE_WORDS = { view: "who can watch", operate: "who can run the show", admin: "an admin" };
const HOW_TO_ACCOUNTS = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/sign-in-with-a-username-and-password.md";

/** The dialog. */
export async function openSignIn() {
  const desktop = /GodwinMix-Desktop/.test(navigator.userAgent);
  const { authStatus, signOut } = await import("../client/auth.js");
  const account = await authStatus(location.origin, storedToken());
  if (!account.accounts) {
    const body = el("div.col", {}, signInParts(location.origin, storedToken(), desktop));
    return modal({ title: "Address and token", body });
  }
  const actions = {
    signOut: async () => {
      await signOut(location.origin);
      location.reload();
    },
    changePassword: () => import("./password.js").then((p) => p.openChangePassword(window.gmxClient)),
  };
  const body = el("div.col", {}, accountParts(location.origin, storedToken(), account, actions));
  return modal({ title: "Address and sign in", body });
}
