// Step one: the camera strip. A card per camera, painted about fifteen times
// a second while the window is on screen and not at all while it is not.

import { el } from "../../shell/dom.js";
import { card } from "./card.js";
import { pictures, drawPeople } from "./picture.js";
import { tallyOf } from "./words.js";

const FRAME_MS = 66;

function colours(node) {
  const css = getComputedStyle(node);
  const follow = css.getPropertyValue("--ok").trim() || "#34d07f";
  return { follow, followFill: "rgba(52,208,127,0.12)", onFollow: "#06140c" };
}

export function strip(model, on) {
  const list = el("div.cam-strip", { role: "group", "aria-label": "Cameras" });
  const empty = el("div.cam-empty", {}, [
    el("p", { text: "No cameras yet." }),
    el("p.dim", { text: "Add a PTZ camera with the ptz plugin, or try this window with three pretend cameras." }),
    el("button.cam-try", { type: "button", text: "Try the demo", onclick: () => on.demo() }),
  ]);
  const root = el("section.cam-step.cam-step-pick", {}, [el("h3.cam-steptitle", {}, [el("span.cam-stepno", { text: "1" }), "Pick what's next"]), list, empty]);
  let cards = new Map();
  let pics = pictures(model.api.client);
  let raf = 0;
  let last = 0;
  let tint = null;
  let built = null;

  function build() {
    empty.hidden = model.cams.length > 0 || !(model.loaded || model.error);
    const ids = model.cams.map((c) => c.id + "=" + c.name).join();
    if (ids === built) return;
    built = ids;
    cards = new Map(model.cams.map((c) => [c.id, card(c, on)]));
    list.replaceChildren(...[...cards.values()].map((c) => c.root));
  }

  let shown = null;
  /** On a phone the strip scrolls sideways; keep the next camera in view. */
  function reveal() {
    const c = cards.get(model.next);
    if (!c || shown === model.next || list.scrollWidth <= list.clientWidth) return;
    shown = model.next;
    list.scrollTo({ left: c.root.offsetLeft - (list.clientWidth - c.root.clientWidth) / 2, behavior: "smooth" });
  }

  function sync() {
    build();
    reveal();
    for (const [id, c] of cards) {
      c.sync({
        tally: tallyOf(c.cam, model.mix, model.atem, model.next),
        track: model.track.get(id),
        people: model.seen.get(id),
        next: model.next === id,
      });
    }
  }

  function paint(t) {
    raf = requestAnimationFrame(paint);
    if (t - last < FRAME_MS) return;
    last = t;
    tint = tint || colours(root);
    for (const c of cards.values()) {
      if (!c.root.isConnected) continue;
      pics.draw(c.cam, c.canvas);
      drawPeople(c.canvas.getContext("2d"), c.canvas.width, c.canvas.height, c.seen, tint);
    }
  }

  return {
    root,
    sync,
    cards: () => cards,
    start() {
      if (!raf) raf = requestAnimationFrame(paint);
    },
    stop() {
      cancelAnimationFrame(raf);
      raf = 0;
    },
    reset() {
      pics = pictures(model.api.client);
      cards = new Map();
      built = null;
      tint = null;
      sync();
    },
    retheme: () => (tint = null),
  };
}
