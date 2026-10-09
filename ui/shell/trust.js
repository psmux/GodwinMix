// Trusting this mixer on a phone. The mixer makes its own certificate
// authority once per machine and signs its https certificate with it; a phone
// that installs the authority stops warning about the certificate, and Chrome
// on Android will then offer to install the page as an app. The authority's
// certificate is public and served at /ca.crt (iOS opens it as a profile) and
// /ca.pem (a plain download, for Android's settings to pick up).

import { el } from "./dom.js";

export const TRUST_HELP = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/install-as-an-app.md#trust-this-mixer-on-a-phone";

/** Where a phone downloads the authority, on the mixer at `base`. */
export function authorityLinks(base) {
  const root = base.replace(/#.*$/, "").replace(/\/?$/, "/");
  return { apple: root + "ca.crt", android: root + "ca.pem" };
}

/** A few lines for the Open on another device card, or null when the mixer has no authority of its own. */
export function trustNote(info, base) {
  const print = info && info.tls && info.tls.authority;
  if (!print || !base) return null;
  const links = authorityLinks(base);
  const link = (href, text) => el("a", { href, target: "_blank", rel: "noopener", text });
  return el("div.col", {}, [
    el("strong", { text: "Trust this mixer on your phone" }),
    el("p.sm", {}, [
      "Install the mixer's certificate authority on the phone once, and it stops warning about the certificate. ",
      "On the phone, open ", link(links.apple, links.apple), " on an iPhone or iPad, or ",
      link(links.android, links.android), " on Android, then follow ", link(TRUST_HELP, "these steps"), ".",
    ]),
    // Wrapped anywhere: a fingerprint is one long word that pushed a phone's dialog sideways.
    el("p.sm.dim", { style: { overflowWrap: "anywhere" }, text: `Authority fingerprint, to compare with what the phone shows: ${print}` }),
  ]);
}
