// One card in the gallery: its picture, its name, what it is, and the one
// button that does the next thing (Add, then Take live, then Take off).
//
// The picture is not asked for until the card scrolls into view, so a
// gallery of a hundred items opened on a phone draws the dozen on screen.
// A moving item plays its clip while a pointer rests on it, or after a
// long press on a touch screen, and stops when it leaves.

import { el } from "../../shell/dom.js";
import { contextMenu } from "../../shell/menu.js";
import { errorToast } from "../../shell/toast.js";
import { label, badges, previewUrl, fileUrl, nextStep } from "./model.js";
import * as act from "./actions.js";

/** The card for `item`. `host` is the gallery: its client, its state per id, and reload. */
export function card(host, item) {
  const pic = el("div.gx-pic", { title: item.description || item.name });
  const img = el("img", { alt: `${item.name}, ${label(item)}`, decoding: "async", loading: "lazy" });
  img.dataset.src = previewUrl(host.client, item, 480);
  pic.appendChild(img);
  const main = el("button.btn.primary.gx-main");
  const node = el("div.gx-card", { "data-id": item.id }, [
    pic,
    el("div.gx-meta", {}, [
      el("div.gx-name.ellipsis", { text: item.name }),
      el("div.gx-label.sm.dim", { text: [label(item), ...badges(item)].join(" · ") }),
    ]),
    el("div.gx-actions", {}, [
      main,
      item.fields && item.fields.length ? el("button.btn", { text: "Edit", onclick: () => host.edit(item) }) : null,
      el("button.btn.icon", { text: "⋯", title: "More", "aria-label": `More for ${item.name}`, onclick: (e) => more(host, item, e) }),
    ]),
  ]);
  paint(main, item, host.state.get(item.id));
  main.onclick = () => step(host, item, main);
  pic.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    play(host, item, pic);
  });
  pic.addEventListener("pointerenter", (e) => e.pointerType === "mouse" && play(host, item, pic));
  pic.addEventListener("pointerleave", () => stop(pic));
  return node;
}

function paint(button, item, state) {
  const next = nextStep(item, state);
  button.hidden = !next;
  if (next) button.textContent = next.label;
}

/** Do what the main button says, and move it on to the next step. */
async function step(host, item, button) {
  const next = nextStep(item, host.state.get(item.id));
  if (!next) return;
  button.disabled = true;
  try {
    if (next.id === "place") await act.place(host.client, item);
    else await act.show(host.client, item, next.id === "show");
    host.state.set(item.id, next.id === "place" ? "placed" : next.id === "show" ? "shown" : "placed");
  } catch (e) {
    errorToast(e, `${next.label} ${item.name}`);
  } finally {
    button.disabled = false;
    paint(button, item, host.state.get(item.id));
  }
}

function more(host, item, e) {
  const r = e.currentTarget.getBoundingClientRect();
  const guard = (fn, what) => () => fn().catch((err) => errorToast(err, what));
  contextMenu(r.left, r.bottom + 4, [
    { label: "Add to the scene on air", run: guard(() => act.place(host.client, item).then(() => host.state.set(item.id, "placed")), "Add") },
    { label: "Duplicate", run: guard(() => act.duplicate(host.client, item).then(() => host.reload()), "Duplicate") },
    { label: "Export as a zip", run: guard(() => act.exportItems(host.client, [item.id]), "Export") },
    { kind: "separator" },
    { label: "Delete", disabled: item.origin === "shipped", run: guard(() => act.remove(host.client, item).then((gone) => gone && host.reload()), "Delete") },
  ]);
}

/** The clip or the item's own loop, over its picture. */
function play(host, item, pic) {
  if (!item.moving || pic.querySelector("video")) return;
  const video = el("video", { muted: true, loop: true, autoplay: true, playsInline: true, src: fileUrl(host.client, item, item.moving) });
  video.muted = true;
  pic.appendChild(video);
  video.play?.().catch(() => video.remove());
}

function stop(pic) {
  pic.querySelector("video")?.remove();
}

/** Ask for the pictures of the cards now on screen, and only those. */
export function watchPictures(root) {
  if (typeof IntersectionObserver === "undefined") {
    for (const img of root.querySelectorAll("img[data-src]")) img.src = img.dataset.src;
    return { disconnect() {} };
  }
  const seen = new IntersectionObserver((entries) => {
    for (const e of entries) {
      if (!e.isIntersecting) continue;
      const img = e.target;
      if (img.dataset.src && img.src !== img.dataset.src) img.src = img.dataset.src;
      seen.unobserve(img);
    }
  }, { root: null, rootMargin: "200px" });
  for (const img of root.querySelectorAll("img[data-src]")) seen.observe(img);
  return seen;
}
