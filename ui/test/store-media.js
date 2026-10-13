// The carousel on its own: a slide, a dot and arrows per picture and clip,
// the arrow keys, GIFs that only play while seen, and reduced motion.

import { carousel } from "../panels/plugins/media.js";
import { plugins } from "./plugins-stub.js";

const settle = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 3000) => { for (let t = 0; t < ms; t += 20) { if (fn()) return true; await settle(20); } return false; };

export const GIF = "data:image/gif;base64,R0lGODlhAQABAAAAACw=";
export const PNG = "data:image/gif;base64,R0lGODlhAQABAAAAACw=#still";
export const MP4 = "data:video/mp4;base64,AAAA";

/** Pretend the person asked for less motion, or not. */
export function motion(less) {
  const real = window.matchMedia.bind(window);
  window.matchMedia = (q) => (q.includes("reduced-motion") ? { matches: less, media: q, addEventListener() {}, removeEventListener() {} } : real(q));
  return () => { window.matchMedia = real; };
}

export function shiny() {
  const base = plugins()[1];
  const at = (days) => new Date(Date.now() - days * 864e5).toISOString();
  return [
    { ...base, name: "scripture", install: "scripture", title: "Scripture", featured: true, updated: at(1), kinds: ["panel", "graphic"], preview: GIF,
      media: [{ url: GIF, type: "animation" }, { url: PNG, type: "image" }, { url: MP4, type: "video" }], thumbnail: PNG },
    { ...base, name: "hls-player", install: "hls-player", title: "HLS player", featured: true, updated: at(3), kinds: ["source"], thumbnail: PNG },
    { ...base, name: "social-chat", install: "social-chat", title: "Social chat", updated: at(9), kinds: ["panel"], thumbnail: PNG },
  ];
}

export async function carouselTests(test, eq, ok) {
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
