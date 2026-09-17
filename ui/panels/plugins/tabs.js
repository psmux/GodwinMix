// The two tabs, and the switch between them.
//
// Installed is what is on the machine; Get more is where the rest comes from.
// Both are built when the section first opens and only the one on screen is
// refreshed, so a search does not run every time somebody looks at the list.

import { el } from "../../shell/dom.js";
import { installedTab } from "./installed.js";
import { marketTab } from "./market.js";

const TABS = [
  { id: "installed", title: "Installed", make: installedTab },
  { id: "market", title: "Get more", make: marketTab },
];

/** Build the panel body into `host`. Answers the handle the element keeps. */
export function mountTabs(client, host) {
  const bar = el("div.plugin-tabs", { role: "tablist" });
  const body = el("div.plugin-body");
  host.append(bar, body);

  const made = new Map();
  let current = "";

  function show(id) {
    if (current === id) return;
    current = id;
    for (const button of bar.children) {
      const on = button.dataset.tab === id;
      button.classList.toggle("on", on);
      button.setAttribute("aria-selected", on ? "true" : "false");
    }
    let tab = made.get(id);
    if (!tab) {
      tab = TABS.find((t) => t.id === id).make(client);
      made.set(id, tab);
    }
    body.textContent = "";
    body.appendChild(tab.node);
    // Every time, not only the first: a plugin installed on the other tab is
    // the reason somebody comes back to this one.
    tab.refresh();
  }

  for (const tab of TABS) {
    bar.appendChild(
      el("button.btn.plugin-tab", {
        text: tab.title,
        role: "tab",
        "data-tab": tab.id,
        onclick: () => show(tab.id),
      })
    );
  }
  show("installed");

  return {
    show,
    destroy() {
      for (const tab of made.values()) tab.destroy();
      made.clear();
    },
  };
}
