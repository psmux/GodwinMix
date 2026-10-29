// /join/ on its own: the publisher in a page of its own, told where to
// publish by its link. Two kinds of link:
//
//   /join/#whip=/whip/browser/laptop&key=<the channel key>&title=Laptop camera
//   /join/#channel=browser&key=<the channel key>
//
// The first names the stream. The second names only the channel, which is
// the link the mixer's "A phone's camera" shows as a QR code: every phone
// that scans it publishes under a name of its own, so each becomes its own
// source, and the person holding it can rename it before going live.
//
// A third kind comes from a guest invite (`guest.invite.create`):
//
//   /join/#guest=ana-lopez&key=<the invite's own key>&title=Ana%20Lopez
//
// It publishes to /guest/<id>/whip as the source guest-<id>, and plays the
// guest's return under the camera once they are live: the programme picture
// with everyone's sound but theirs. `show=` names the show under a station.
//
// Everything is in the fragment, which a browser never sends to a server, so
// the key does not land in a proxy's access log. No operator token is needed:
// the channel key, or the invite's, is what lets a publisher in.

import { mountPublisher } from "./publisher.js";
import { nameField } from "./name-field.js";
import { GuestReturn } from "./return.js";

/** The link's fragment as `{url, key, title, channel, shape, guest, show}`. Missing parts are "". */
export function parseLink(hash) {
  const q = new URLSearchParams(String(hash || "").replace(/^#/, ""));
  const get = (k) => q.get(k) || "";
  return { url: get("whip"), key: get("key"), title: get("title"), channel: get("channel"), shape: get("shape"), guest: get("guest"), show: get("show") };
}

/** A guest's two addresses: where the camera goes and where the return comes from. */
export function guestUrls(guest, show) {
  const tail = show ? `?show=${encodeURIComponent(show)}` : "";
  const at = (kind) => `/guest/${encodeURIComponent(guest)}/${kind}${tail}`;
  return { whip: at("whip"), whep: at("whep") };
}

/** The WHIP address for a stream on a channel. */
export function whipUrl(channel, stream) {
  return `/whip/${encodeURIComponent(channel)}/${encodeURIComponent(stream)}`;
}

const root = document.getElementById("join");
if (root) start(root, parseLink(location.hash));

function start(root, link) {
  if (link.title) document.title = link.title;
  const heading = document.getElementById("join-title");
  if (heading && link.title) heading.textContent = link.title;
  if (link.guest && link.key) return startGuest(root, link);
  if (!link.key || (!link.url && !link.channel)) {
    root.textContent = "This link has no address or no key in it, so there is nowhere to publish. Ask whoever sent it for the whole link.";
    return;
  }
  let url = link.url;
  let where = link.url && `Publishing to ${new URL(link.url, location.href).pathname.replace(/^\/whip\//, "")}`;
  let onState;
  if (!url) {
    const field = nameField(link.channel);
    root.appendChild(field.node);
    url = () => whipUrl(link.channel, field.stream());
    where = "";
    onState = (s) => field.lock(s.state === "connecting" || s.state === "live" || s.state === "reconnecting");
  }
  const pub = mountPublisher(root, { url, key: link.key, keepAwake: true, onState, labels: { where }, shape: { link: link.shape } });
  addEventListener("beforeunload", (e) => {
    if (pub.active()) e.preventDefault();
  });
  addEventListener("pagehide", () => pub.stop());
}

/** An invited guest: the camera, named for them, and the show under it. */
function startGuest(root, link) {
  const urls = guestUrls(link.guest, link.show);
  const heading = document.getElementById("join-title");
  if (heading) heading.textContent = link.title ? `Join the show as ${link.title}` : "Join the show";
  const back = { follow: () => {} };
  const labels = { where: "", go: "Join the show" };
  const pub = mountPublisher(root, { url: urls.whip, key: link.key, keepAwake: true, labels, onState: (s) => back.follow(s.state) });
  const ret = new GuestReturn(root, { url: urls.whep, key: link.key });
  back.follow = (state) => ret.follow(state);
  addEventListener("beforeunload", (e) => {
    if (pub.active()) e.preventDefault();
  });
  addEventListener("pagehide", () => {
    ret.stop();
    pub.stop();
  });
}
