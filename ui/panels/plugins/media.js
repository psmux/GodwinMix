// A plugin's pictures and clips: the hero carousel in the detail view, and
// the clip a card plays while a mouse is over it.
//
// Nothing moves unless somebody is looking. A GIF is only given its address
// while its slide is on screen, since an <img> cannot pause one and taking
// the address away is what stops it decoding. A video is muted, loops,
// loads nothing until its slide shows, and pauses when it leaves. With
// prefers-reduced-motion nothing starts by itself: a clip waits behind a
// Play button, and a card plays nothing on hover.

import { el, svg } from "../../shell/dom.js";

const ARROW = { prev: "M15 5l-7 7 7 7", next: "M9 5l7 7-7 7" };

/** Whether this person asked for less motion. */
export function reduced() {
  return !!(window.matchMedia && window.matchMedia("(prefers-reduced-motion: reduce)").matches);
}

/** The carousel's items: `media` when the core sent it, else the screenshots. */
export function itemsOf(p) {
  const media = (p.media || []).filter((m) => m && m.url);
  if (media.length) return media;
  const shots = (p.screenshots || []).length ? p.screenshots : p.thumbnail ? [p.thumbnail] : [];
  return shots.map((url) => ({ url, type: /\.gif($|[?#])/i.test(url) ? "animation" : "image" }));
}

/** One slide: a picture, an animation that waits for its turn, or a video. */
function slide(item, i, count, title) {
  const label = `${title}, ${i + 1} of ${count}`;
  if (item.type === "video") {
    const v = el("video.pg-clip", { muted: true, loop: true, playsInline: true, preload: "none", "aria-label": label });
    v.dataset.src = item.url;
    v.muted = true;
    return el("div.pg-slide", { "data-type": "video" }, [v]);
  }
  const img = el("img.pg-clip", { alt: label, decoding: "async", referrerpolicy: "no-referrer", loading: i ? "lazy" : "eager" });
  if (item.type === "animation") img.dataset.src = item.url;
  else img.src = item.url;
  return el("div.pg-slide", { "data-type": item.type || "image" }, [img]);
}

/** Start or stop what is on one slide. */
function live(node, on, still) {
  const clip = node.querySelector(".pg-clip");
  if (!clip || !clip.dataset.src) return;
  const type = node.dataset.type;
  if (type === "animation") {
    const want = on && !still() ? clip.dataset.src : "";
    if (want && clip.getAttribute("src") !== want) clip.src = want;
    if (!want && clip.hasAttribute("src")) clip.removeAttribute("src");
  } else if (type === "video") {
    if (on && !still()) {
      if (!clip.src) clip.src = clip.dataset.src;
      clip.play?.().catch(() => {});
    } else clip.pause?.();
  }
  node.classList.toggle("waiting", on && still() && type !== "image");
}

/**
 * The hero carousel: arrows, dots, a swipe (the track scrolls with snap
 * points, so touch and a trackpad swipe it natively) and the arrow keys.
 */
export function carousel(p) {
  const items = itemsOf(p);
  const track = el("div.pg-track", { tabindex: "0", "aria-label": `${p.title} pictures` });
  const slides = items.map((item, i) => slide(item, i, items.length, p.title));
  track.append(...slides);
  const dots = el("div.pg-dots", { role: "tablist" });
  let at = 0;
  const playing = new Set();
  const forced = new Set();
  const still = (i) => () => reduced() && !forced.has(i);
  const go = (i) => {
    const n = Math.max(0, Math.min(slides.length - 1, i));
    track.scrollTo({ left: n * track.clientWidth, behavior: reduced() ? "auto" : "smooth" });
  };
  const mark = (i) => {
    at = i;
    [...dots.children].forEach((d, j) => d.setAttribute("aria-selected", String(j === i)));
    prev.disabled = i === 0;
    next.disabled = i === slides.length - 1;
  };
  slides.forEach((_, i) => dots.append(el("button.pg-dot", { type: "button", role: "tab", "aria-label": `Show ${i + 1} of ${slides.length}`, onclick: () => go(i) })));
  const prev = el("button.pg-arrow.prev", { type: "button", "aria-label": "Previous", onclick: () => go(at - 1) }, [svg(ARROW.prev, 22)]);
  const next = el("button.pg-arrow.next", { type: "button", "aria-label": "Next", onclick: () => go(at + 1) }, [svg(ARROW.next, 22)]);
  const play = (i) => { forced.add(i); live(slides[i], true, still(i)); };
  slides.forEach((s, i) => s.addEventListener("click", () => s.classList.contains("waiting") && play(i)));
  track.addEventListener("keydown", (e) => {
    if (e.key === "ArrowRight") { e.preventDefault(); go(at + 1); }
    if (e.key === "ArrowLeft") { e.preventDefault(); go(at - 1); }
  });
  const seen = new IntersectionObserver((entries) => {
    for (const e of entries) {
      const i = slides.indexOf(e.target);
      const on = e.isIntersecting && e.intersectionRatio >= 0.6;
      if (on) { playing.add(i); mark(i); } else playing.delete(i);
      live(e.target, on, still(i));
    }
  }, { root: track, threshold: [0, 0.6] });
  slides.forEach((s) => seen.observe(s));
  const root = el("div.pg-hero", { "data-count": String(slides.length) }, [track, prev, next, dots]);
  if (slides.length < 2) root.classList.add("single");
  mark(0);
  // Off screen altogether, a dialog scrolled past it, nothing plays.
  const shown = new IntersectionObserver(([e]) => {
    for (const i of playing) live(slides[i], e.isIntersecting, still(i));
  });
  shown.observe(root);
  root.stop = () => { seen.disconnect(); shown.disconnect(); slides.forEach((s) => live(s, false, () => true)); };
  root.go = go;
  return root;
}

/**
 * Play a card's preview while a mouse is over its picture or the card has
 * the keyboard's focus. Nothing is fetched until then, and it is thrown away
 * when the pointer leaves.
 */
export function hoverPreview(card, p) {
  const url = p.preview;
  if (!url) return;
  const video = /\.(mp4|webm)($|[?#])/i.test(url) || url.startsWith("data:video/");
  let clip = null;
  const start = (e) => {
    // Looked up now: a picture that failed to load was swapped for an icon.
    const pic = card.querySelector(".pg-open .pg-pic");
    if (!pic || clip || reduced() || (e && e.pointerType === "touch")) return;
    clip = video
      ? el("video.pg-pic.pg-moving", { muted: true, loop: true, autoplay: true, playsInline: true, src: url, "aria-hidden": "true" })
      : el("img.pg-pic.pg-moving", { src: url, alt: "", "aria-hidden": "true", referrerpolicy: "no-referrer" });
    if (video) clip.muted = true;
    pic.after(clip);
    card.classList.add("previewing");
  };
  const stop = () => {
    if (!clip) return;
    clip.pause?.();
    clip.remove();
    clip = null;
    card.classList.remove("previewing");
  };
  card.addEventListener("pointerenter", start);
  card.addEventListener("pointerleave", stop);
  card.addEventListener("focusin", () => start());
  card.addEventListener("focusout", (e) => !card.contains(e.relatedTarget) && stop());
}
