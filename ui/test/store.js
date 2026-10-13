// The Plugins page as an app store, against the stub: the words an install
// shows as it runs, the Featured and New and updated rows, the carousel with
// its arrows, dots and keys, GIFs that only play while seen, a card's clip on
// hover, reduced motion, and a failed install's next step.

import { busyLabel } from "../panels/plugins/install.js";
import { featured, fresh } from "../panels/plugins/rows.js";
import { itemsOf } from "../panels/plugins/media.js";
import { carouselTests, motion, shiny, GIF } from "./store-media.js";
import { nextStep } from "../panels/plugins/act.js";
import { primary } from "../panels/plugins/model.js";
import { openGallery } from "../panels/plugins/view.js";
import { registerSandboxed } from "../shell/registry.js";
import { GalleryStub } from "./plugins-stub.js";

const until = async (fn, ms = 3000) => {
  for (let t = 0; t < ms; t += 20) {
    if (fn()) return true;
    await new Promise((r) => setTimeout(r, 20));
  }
  return false;
};

export async function storeTests(test, eq, ok) {
  test("store: an install says Downloading with a percent, then Installing, then Starting", () => {
    const p = { bundled: false };
    eq(busyLabel(null, "add", p).label, "Downloading");
    eq(busyLabel(null, "add", { bundled: true }).label, "Installing");
    eq(busyLabel({ stage: "downloading", progress: 0.42 }, "add", p), { label: "Downloading 42%", pct: 42 });
    eq(busyLabel({ stage: "downloading" }, "add", p), { label: "Downloading", pct: null }, "no size, no percent");
    eq(busyLabel({ stage: "installing", progress: 0.85 }, "add", p).label, "Installing");
    eq(busyLabel({ stage: "starting", progress: 0.95 }, "add", p).label, "Starting");
    eq(primary({ installed: true, runs_here: true }, true), { label: "Open", act: "open" });
  });

  test("store: Featured takes what the index marked, New and updated goes by date", () => {
    const list = shiny();
    eq(featured(list).map((p) => p.name), ["scripture", "hls-player"]);
    eq(fresh(list).map((p) => p.name), ["scripture", "hls-player", "social-chat"]);
    eq(featured(list.slice(0, 1)).map((p) => p.name), ["scripture"], "one marked is a row");
    eq(featured([{ ...list[2], featured: false }]), [], "one picked for its picture is not");
  });

  test("store: a failed install names the next step, the core's own when it wrote one", () => {
    eq(nextStep("add", new Error("`x` has no asset for linux-armv7. Ask the author to add this platform.")), "Ask the author to add this platform.");
    ok(nextStep("add", new Error("asking https://api.github.com timed out")).includes("reach the internet"));
    ok(nextStep("add", new Error("it broke.")).startsWith("Press Try again."));
  });

  test("store: an index from 0.3.8 still fills the carousel from its screenshots", () => {
    eq(itemsOf({ screenshots: ["https://x/a.png", "https://x/b.gif"] }).map((m) => m.type), ["image", "animation"]);
    eq(itemsOf({ thumbnail: "https://x/t.png" }).length, 1);
  });

  await carouselTests(test, eq, ok);
  await pageTests(test, eq, ok);
}

async function pageTests(test, eq, ok) {
  const undo = motion(false);
  const stub = new GalleryStub({ stages: [{ stage: "downloading", progress: 0.3 }, { stage: "installing", progress: 0.85 }, { stage: "starting", progress: 0.95 }] });
  stub.list = shiny();
  const arrived = [];
  // What /plugins/index.json would bring after the install: one sandboxed panel.
  const arrive = async (p) => {
    arrived.push(p.name);
    registerSandboxed({ id: `${p.name}/panel`, title: p.title, src: "about:blank", plugin: p.name });
    return [`${p.name}/panel`];
  };
  const view = openGallery(stub, { arrive });
  await until(() => view.root.querySelector(".pg-row"));
  test("store: on All the page opens with Featured and New and updated", () => {
    eq([...view.root.querySelectorAll(".pg-row h3")].map((h) => h.textContent), ["Featured", "New and updated"]);
    eq(view.root.querySelectorAll(".pg-feature").length, 2);
  });
  const card = view.root.querySelector('.pg-grid .pg-card[data-name="scripture"]');
  card.dispatchEvent(new PointerEvent("pointerenter", { pointerType: "mouse" }));
  test("store: a card plays its clip while the mouse is over it, and nothing before", () => {
    eq(card.querySelector(".pg-moving")?.getAttribute("src"), GIF);
  });
  card.dispatchEvent(new PointerEvent("pointerleave", { pointerType: "mouse" }));
  test("store: and drops it when the mouse leaves", () => ok(!card.querySelector(".pg-moving")));

  const seen = new Set();
  const watch = setInterval(() => { for (const b of view.root.querySelectorAll('[data-name="scripture"] .pg-go')) seen.add(b.textContent); }, 15);
  card.querySelector(".pg-go").click();
  await until(() => view.root.querySelector('.pg-grid [data-name="scripture"] .pg-go')?.textContent === "Open", 5000);
  clearInterval(watch);
  test("store: the button goes Downloading 30%, Installing, Starting, then Open", () => {
    for (const word of ["Downloading 30%", "Installing", "Starting"]) ok(seen.has(word), `${word} in ${[...seen].join(", ")}`);
    eq(view.root.querySelector('.pg-grid [data-name="scripture"] .pg-go').textContent, "Open");
  });
  test("store: after the install its panel is found and opened, with no other step", () => {
    eq(arrived, ["scripture"]);
    ok(view.root.querySelector('.pg-grid [data-name="scripture"] .pg-note').textContent.includes("Its panel is open"));
  });
  search(view, "hls");
  test("store: a search hides the rows", () => ok(!view.root.querySelector(".pg-row")));
  view.close();
  undo();
}

function search(view, words) {
  const box = view.root.querySelector(".pg-search");
  box.value = words;
  box.dispatchEvent(new Event("input"));
}
