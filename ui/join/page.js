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
// Everything is in the fragment, which a browser never sends to a server, so
// the key does not land in a proxy's access log. No operator token is needed:
// the channel key is what lets a publisher in.

import { mountPublisher } from "./publisher.js";
import { nameField } from "./name-field.js";

/** The link's fragment as `{url, key, title, channel}`. Missing parts are "". */
export function parseLink(hash) {
  const q = new URLSearchParams(String(hash || "").replace(/^#/, ""));
  return { url: q.get("whip") || "", key: q.get("key") || "", title: q.get("title") || "", channel: q.get("channel") || "" };
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
  const pub = mountPublisher(root, { url, key: link.key, keepAwake: true, onState, labels: { where } });
  addEventListener("beforeunload", (e) => {
    if (pub.active()) e.preventDefault();
  });
  addEventListener("pagehide", () => pub.stop());
}
