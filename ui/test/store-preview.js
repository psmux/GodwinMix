// The Plugins page against the stub, one state per address, for looking at
// and for screenshots: /test/store-preview.html?scene=page&art=<base URL>
//
// Scenes: page, detail, installing, failed. `art` is a folder with
// <plugin>/icon.png, thumbnail.png, screenshot-1.png, screenshot-2.png and
// demo.gif for each plugin below. Nothing here talks to a core.

import { openGallery } from "../panels/plugins/view.js";
import { GalleryStub, plugins } from "./plugins-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "page";
const art = (q.get("art") || "/").replace(/\/?$/, "/");
const wait = (ms = 80) => new Promise((r) => setTimeout(r, ms));
const day = (n) => new Date(Date.now() - n * 864e5).toISOString();

const LISTED = [
  ["scripture", "Scripture", "Verses on screen in one press. Type a reference, see it on Preview, take it live.", ["panel", "graphic"], 312, 4100, 1, true],
  ["hls-player", "HLS player", "Any HLS stream as a source, with its quality picked for the canvas.", ["source"], 188, 2600, 2, true],
  ["social-chat", "Social chat", "YouTube and Twitch chat on a panel, and the message you pick on air.", ["panel", "graphic"], 240, 3900, 4, true],
  ["lower-thirds", "Lower thirds", "Names and titles, styled once and used all show.", ["graphic"], 95, 1200, 12, false],
  ["countdown", "Countdown", "A clock to the start of the show.", ["source"], 41, 640, 30, false],
];

function gallery() {
  const base = plugins()[1];
  const made = LISTED.map(([name, title, description, kinds, stars, downloads, age, featured]) => {
    const at = (f) => `${art}${name}/${f}`;
    return {
      ...base, name, install: name, title, description, kinds, stars, downloads, updated: day(age), featured,
      author: "psmux", license: "MIT", tags: [], icon: at("icon.png"), thumbnail: at("thumbnail.png"), preview: at("demo.gif"),
      media: [{ url: at("demo.gif"), type: "animation" }, { url: at("screenshot-1.png"), type: "image" }, { url: at("screenshot-2.png"), type: "image" }],
      repository: `https://github.com/psmux/godwinmix-${name}`, fit: "Runs here", fit_detail: "One download runs on every computer, so it installs here.",
    };
  });
  return [...made, ...plugins().filter((p) => p.name !== "clock")];
}

async function main() {
  document.documentElement.dataset.theme = q.get("theme") || "dark";
  const forever = Array.from({ length: 400 }, () => ({ stage: "downloading", progress: 0.42 }));
  const stub = new GalleryStub({
    stages: scene === "installing" ? forever : [],
    fail: scene === "failed" ? { "hls-player": "hls-player has no asset for windows-x86_64. Ask the author to add this platform, or one asset for any platform." } : {},
  });
  stub.list = gallery();
  const view = openGallery(stub, { arrive: async () => [] });
  await wait(300);
  if (scene === "detail") view.root.querySelector('.pg-feature[data-name="scripture"] .pg-open').click();
  if (scene === "installing" || scene === "failed") {
    const name = scene === "installing" ? "scripture" : "hls-player";
    view.root.querySelector(`.pg-grid [data-name="${name}"] .pg-go`).click();
    await wait(1500);
    if (scene === "installing") view.root.querySelector(".pg-scroll").scrollTop = 9999;
    else view.root.querySelector(`.pg-grid [data-name="${name}"]`).scrollIntoView();
  }
  await wait(800);
  // The pictures on screen, drawn, before anybody takes a screenshot.
  const drawn = [...document.images].filter((i) => i.getAttribute("src") && i.getBoundingClientRect().top < innerHeight);
  await Promise.race([Promise.all(drawn.map((i) => i.decode().catch(() => {}))), wait(5000)]);
  await wait(300);
  document.body.dataset.ready = "1";
}

main();
