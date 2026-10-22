// The headend's stylesheets, each linked once. The promise settles when
// the sheet has loaded, so nothing is measured before it has a layout.

import { el } from "../../shell/dom.js";

const loading = new Map();

export function sheet(name) {
  if (loading.has(name)) return loading.get(name);
  const link = el(`link#gmx-headend-${name}`, { rel: "stylesheet", href: new URL(`./${name}.css`, import.meta.url).href });
  const done = new Promise((resolve) => {
    link.addEventListener("load", resolve, { once: true });
    link.addEventListener("error", resolve, { once: true });
  });
  document.head.append(link);
  loading.set(name, done);
  return done;
}
