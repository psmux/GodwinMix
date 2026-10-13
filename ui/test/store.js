// The Plugins page as an app store, against the stub: the words an install
// shows as it runs, the Featured and New and updated rows, the carousel with
// its arrows, dots and keys, GIFs that only play while seen, a card's clip on
// hover, reduced motion, and a failed install's next step.

import { busyLabel } from "../panels/plugins/install.js";
import { featured, fresh } from "../panels/plugins/rows.js";
import { itemsOf, carousel } from "../panels/plugins/media.js";
import { nextStep } from "../panels/plugins/act.js";
import { primary } from "../panels/plugins/model.js";
import { openGallery } from "../panels/plugins/view.js";
import { registerSandboxed } from "../shell/registry.js";
import { GalleryStub, plugins } from "./plugins-stub.js";

const settle = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 3000) => { for (let t = 0; t < ms; t += 20) { if (fn()) return true; await settle(20); } return false; };
const GIF = "data:image/gif;base64,R0lGODlhAQABAAAAACw=";
const PNG = "data:image/gif;base64,R0lGODlhAQABAAAAACw=#still";
const MP4 = "data:video/mp4;base64,AAAA";

/** Pretend the person asked for less motion, or not. */
function motion(less) {
  const real = window.matchMedia.bind(window);
  window.matchMedia = (q) => (q.includes("reduced-motion") ? { matches: less, media: q, addEventListener() {}, removeEventListener() {} } : real(q));
  return () => { window.matchMedia = real; };
}

function shiny() {
  const base = plugins()[1];
  const at = (days) => new Date(Date.now() - days * 864e5).toISOString();
  return [
    { ...base, name: "scripture", install: "scripture", title: "Scripture", featured: true, updated: at(1), kinds: ["panel", "graphic"], preview: GIF,
      media: [{ url: GIF, type: "animation" }, { url: PNG, type: "image" }, { url: MP4, type: "video" }], thumbnail: PNG },
    { ...base, name: "hls-player", install: "hls-player", title: "HLS player", featured: true, updated: at(3), kinds: ["source"], thumbnail: PNG },
    { ...base, name: "social-chat", install: "social-chat", title: "Social chat", updated: at(9), kinds: ["panel"], thumbnail: PNG },
  ];
}

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
    eq(featured(list.slice(0, 1)), [], "one is not a row");
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

async function carouselTests(test, eq, ok) {
  const undo = motion(false);
  const box = document.body.appendChild(Object.assign(document.createElement("div"), { style: "width:480px" }));
  const hero = carousel(shiny()[0]);
  box.append(hero);
  await settle(150);
  const slides = [...hero.querySelectorAll(".pg-slide")];
  const gif = slides[0].querySelector("img");
  test("store: the carousel has a slide, a dot and arrows for each picture and clip", () => {
    eq(slides.map((s) => s.dataset.type), ["animation", "image", "video"]);
    eq(hero.querySelectorAll(".pg-dot").length, 3);
    ok(hero.querySelector(".pg-arrow.prev").disabled && !hero.querySelector(".pg-arrow.next").disabled, "at the start");
    eq(gif.getAttribute("src"), GIF, "the GIF on screen plays");
    ok(!slides[2].querySelector("video").getAttribute("src"), "the video off screen loads nothing yet");
  });
  hero.querySelector(".pg-arrow.next").click();
  await until(() => hero.querySelectorAll(".pg-dot")[1].getAttribute("aria-selected") === "true");
  test("store: the next arrow moves on, the dots follow, and the GIF stops off screen", () => {
    eq(hero.querySelectorAll(".pg-dot")[1].getAttribute("aria-selected"), "true");
    ok(!gif.hasAttribute("src"), "an <img> cannot pause a GIF, so it loses its address");
  });
  hero.querySelector(".pg-track").dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowRight", bubbles: true }));
  await until(() => hero.querySelectorAll(".pg-dot")[2].getAttribute("aria-selected") === "true");
  test("store: the arrow keys move it, and the video gets its address when it shows", () => {
    eq(hero.querySelectorAll(".pg-dot")[2].getAttribute("aria-selected"), "true");
    eq(slides[2].querySelector("video").getAttribute("src"), MP4);
    ok(slides[2].querySelector("video").muted, "muted");
  });
  hero.stop();
  hero.remove();
  undo();

  const calm = motion(true);
  const still = carousel(shiny()[0]);
  box.append(still);
  await settle(150);
  const first = still.querySelector(".pg-slide");
  test("store: with reduced motion nothing plays until Play is pressed", () => {
    ok(!first.querySelector("img").hasAttribute("src"), "no GIF by itself");
    ok(first.classList.contains("waiting"), "a Play button over it");
  });
  first.click();
  await settle(30);
  test("store: pressing Play plays that one", () => {
    eq(first.querySelector("img").getAttribute("src"), GIF);
  });
  still.stop();
  box.remove();
  calm();
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
