// Help > Open on another device: this mixer's address for a phone or a tablet
// on the same network, as a QR code that signs the device in with one scan.
//
// Each code carries a fresh device token (token.create) in the address's
// fragment, `#token=...`. A browser never sends the fragment to the server, so
// the secret stays out of every log on the way, and boot.js moves it into the
// page's storage and off the address bar before anything draws. The list
// under the code takes a device back. Loaded on first use.

import { el } from "./dom.js";
import { modal } from "./modal.js";
import { toast } from "./toast.js";
import { qrPath } from "../panels/channels/qr.js";
import { deviceList } from "./devices-list.js";

const HOW_TO = "https://github.com/psmux/GodwinMix/blob/main/docs/how-to/run-a-show-from-phones.md";
const LOOPBACK = /^https?:\/\/(localhost|127\.[\d.]+|\[::1\])(:|\/|$)/i;

/** The addresses a phone can use: the https ones that are not this machine's own loopback. */
export function lanUrls(info, origin) {
  const urls = (info.tls && info.tls.urls) || [];
  const lan = urls.filter((u) => !LOOPBACK.test(u));
  if (lan.length || urls.length) return lan;
  // No HTTPS on this mixer. A page opened on a LAN address still names one,
  // and a phone can operate over plain http; it cannot use its camera.
  return origin && !LOOPBACK.test(origin) ? [origin.replace(/\/?$/, "/")] : [];
}

/** The address a phone opens: the page with the token in the fragment. */
export function signInUrl(base, token) {
  return `${base.replace(/#.*$/, "").replace(/\/?$/, "/")}#token=${encodeURIComponent(token)}`;
}

function inDesktopApp() {
  return /GodwinMix-Desktop/.test(navigator.userAgent);
}

/** What to do when nothing on the network can reach this mixer. */
function notReachable() {
  const how = inDesktopApp()
    ? "In the GodwinMix menu, turn on \"Let other devices on this network connect\". The mixer restarts once, keeps the same port from then on, and this card shows a code."
    : "Start the mixer bound to the network, for example with --bind 0.0.0.0:8080 or control.bind in the config, with a control token set, then open this card again.";
  return el("div.col", {}, [
    el("p", { text: "This mixer only answers on the machine it runs on, so a phone has nothing to connect to yet." }),
    el("p.dim", { text: how }),
  ]);
}

function qrPicture(url) {
  const qr = qrPath(url);
  if (!qr) return el("p.dim", { text: "The address is too long for a QR code. Copy the link instead." });
  return el("div", {
    style: { width: "240px", maxWidth: "70vw", alignSelf: "center" },
    html: `<svg viewBox="0 0 ${qr.size} ${qr.size}" shape-rendering="crispEdges" role="img" aria-label="QR code that opens this mixer on another device"><rect width="${qr.size}" height="${qr.size}" fill="#fff"/><path d="${qr.d}" fill="#000"/></svg>`,
  });
}

async function copy(text) {
  try {
    await navigator.clipboard.writeText(text);
    toast({ text: "Copied." });
  } catch {
    toast({ text: "This page may not use the clipboard. Long press the link to copy it." });
  }
}

/** The form: which address, which scope, what to call the device. */
function form(urls) {
  const address = el("select", {}, urls.map((u) => el("option", { value: u, text: u })));
  const scope = el("select", {}, [
    el("option", { value: "operate", text: "Operate: take, switch scenes, run the show", selected: true }),
    el("option", { value: "read", text: "Read: watch only" }),
    el("option", { value: "admin", text: "Admin: everything, settings included" }),
  ]);
  const label = el("input", { type: "text", value: "Phone", maxlength: "64", autocomplete: "off" });
  const field = (name, input) => el("div.field", {}, [el("label", {}, [el("span.lbl", { text: name }), input])]);
  const fields = [urls.length > 1 ? field("Address", address) : null, field("Device", label), field("It may", scope)];
  return { node: el("div.form", {}, fields), read: () => ({ url: address.value || urls[0], scope: scope.value, label: label.value }) };
}

function result(url, fingerprint) {
  return el("div.col", {}, [
    qrPicture(url),
    el("p.sm", { text: "Scan it with the phone's camera. The phone warns about the certificate once, because this mixer made its own; accept it and the page opens signed in." }),
    fingerprint ? el("p.sm.dim", { text: `Certificate fingerprint, to compare with what the phone shows: ${fingerprint}` }) : null,
    el("div.row", {}, [el("button.btn.sm", { text: "Copy link", onclick: () => copy(url) })]),
    el("p.sm.dim", { text: "The link signs in whoever opens it. Send it only to the device it is for, and revoke it below when the show is over." }),
  ]);
}

/** The dialog. */
export async function openDevices(client) {
  const info = await client.call("core.info", {}).catch(() => ({}));
  const urls = lanUrls(info, location.origin);
  const body = el("div.col");
  const more = el("a", { href: HOW_TO, target: "_blank", rel: "noopener", text: "More: running a show from phones" });
  if (!urls.length) {
    body.append(notReachable(), more);
    return modal({ title: "Open on another device", body });
  }
  const { node, read } = form(urls);
  const shown = el("div.col");
  const list = deviceList(client);
  const make = el("button.btn.primary", {
    text: "Make a code",
    onclick: async () => {
      const want = read();
      make.disabled = true;
      try {
        const made = await client.call("token.create", { label: want.label, scope: want.scope });
        shown.replaceChildren(result(signInUrl(want.url, made.token), info.tls && info.tls.fingerprint));
        list.refresh();
      } catch (e) {
        shown.replaceChildren(el("p", { text: (e && e.message) || String(e) }));
      } finally {
        make.disabled = false;
      }
    },
  });
  body.append(
    el("p", { text: "Run this mixer from a phone or a tablet on the same network. Each code signs in one device with its own token, which you can take back here at any time." }),
    node,
    el("div.row", {}, [make]),
    shown,
    list.node,
    more,
  );
  return modal({ title: "Open on another device", body });
}
