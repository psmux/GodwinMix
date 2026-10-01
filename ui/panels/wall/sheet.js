// The wall's stylesheets, each linked once, the first time it is wanted.
// The promise settles when the sheet has loaded: until then the wall has no
// height of its own, every row looks on screen, and nothing is fetched.

import { el } from "../../shell/dom.js";

const loading = new Map();

export function sheet(name) {
  if (loading.has(name)) return loading.get(name);
  const link = el(`link#gmx-wall-${name}`, { rel: "stylesheet", href: new URL(`./${name}.css`, import.meta.url).href });
  const done = new Promise((resolve) => {
    link.addEventListener("load", resolve, { once: true });
    link.addEventListener("error", resolve, { once: true });
  });
  document.head.append(link);
  loading.set(name, done);
  return done;
}
