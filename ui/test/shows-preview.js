// Show tabs and the routing view against the stub, one state per address,
// for looking at and for screenshots: /test/shows.html?scene=three
//
// Scenes: one, three, menu, new, routing, routing-filter. Nothing here
// talks to a core.

import { showStub, sundayChannels } from "./shows-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "three";
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;

const wait = (ms = 80) => new Promise((r) => setTimeout(r, ms));

async function header(stub) {
  window.godwinmixPanels ||= [];
  await import("../panels/header/panel.js");
  const h = document.createElement("gmx-header");
  h.setClient(stub);
  document.querySelector(".slot-header").append(h);
  await wait(150);
  return h;
}

async function main() {
  const one = scene === "one";
  const stub = showStub(one ? { shows: [{ id: "main", name: "Sunday service", state: "running", on_air: "Worship" }] } : {});
  stub.state = { ...stub.state, connected: true, scene: "Worship", outputs: [{ id: "youtube", state: "live" }, { id: "facebook", state: "live" }] };
  await header(stub);
  if (scene === "menu") document.querySelector('.showtab[aria-selected="true"]').click();
  if (scene === "new") (await import("../shell/show-file.js")).newShow(stub);
  if (scene.startsWith("routing")) {
    await sundayChannels(stub);
    const view = (await import("../panels/routing/view.js")).toggleRouting(stub);
    if (scene === "routing-filter") {
      await wait(200);
      const f = view.root.querySelector(".rt-filter");
      f.value = q.get("filter") || "youtube";
      f.dispatchEvent(new Event("input"));
    }
  }
  await wait(500);
  document.body.dataset.ready = "1";
}

main();
