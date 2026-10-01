// /join/ on its own: the publisher in a page of its own, told where to
// publish by its link.
//
//   /join/#whip=/whip/browser/laptop&key=<the channel key>&title=Laptop camera
//
// Everything is in the fragment, which a browser never sends to a server, so
// the key does not land in a proxy's access log.

import { mountPublisher } from "./publisher.js";

/** The link's fragment as `{url, key, title}`. Missing parts are "". */
export function parseLink(hash) {
  const q = new URLSearchParams(String(hash || "").replace(/^#/, ""));
  return { url: q.get("whip") || "", key: q.get("key") || "", title: q.get("title") || "" };
}

const root = document.getElementById("join");
if (root) {
  const link = parseLink(location.hash);
  if (link.title) document.title = link.title;
  const heading = document.getElementById("join-title");
  if (heading && link.title) heading.textContent = link.title;
  if (!link.url || !link.key) {
    root.textContent = "This link has no address or no key in it, so there is nowhere to publish. Ask whoever sent it for the whole link.";
  } else {
    const where = `Publishing to ${new URL(link.url, location.href).pathname.replace(/^\/whip\//, "")}`;
    const pub = mountPublisher(root, { url: link.url, key: link.key, labels: { where } });
    addEventListener("beforeunload", (e) => {
      if (pub.active()) e.preventDefault();
    });
    addEventListener("pagehide", () => pub.stop());
  }
}
