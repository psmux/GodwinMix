// Puts the camera window into its element, with its stylesheet, once.

import { el } from "../../shell/dom.js";
import { CameraView } from "./panel.js";

const CSS = [["gmx-cameras-css", "./cameras.css"], ["gmx-cameras-parts-css", "./parts.css"], ["gmx-cameras-switcher-css", "./switcher.css"]];

export function mount(host) {
  for (const [id, file] of CSS) {
    if (!document.getElementById(id)) document.head.appendChild(el("link#" + id, { rel: "stylesheet", href: new URL(file, import.meta.url).href }));
  }
  if (!host.view) host.view = new CameraView(host, host.client);
  if (host.isConnected) host.view.start();
  return host.view;
}
