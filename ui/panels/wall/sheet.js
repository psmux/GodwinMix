// The wall's stylesheets, each linked once, the first time it is wanted.

import { el } from "../../shell/dom.js";

export function sheet(name) {
  const id = `gmx-wall-${name}`;
  if (document.getElementById(id)) return;
  document.head.append(el(`link#${id}`, { rel: "stylesheet", href: new URL(`./${name}.css`, import.meta.url).href }));
}
