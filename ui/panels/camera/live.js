// A camera's picture with the people the tracker sees boxed over it; tap a
// person to follow them. Camera properties draws the picture too. The Preview
// overlay draws the boxes alone, on a clear canvas over the Preview picture,
// unless the demo is on: then Preview shows the pretend camera, so the boxes
// match what is under them.
//
// It paints about fifteen times a second while it is on the page and stops by
// itself once it is not, so closing the drawer needs nobody to remember.

import { el } from "../../shell/dom.js";
import { pictures, drawPeople, personAt } from "./picture.js";

const FRAME_MS = 66;

function colours(node) {
  const follow = getComputedStyle(node).getPropertyValue("--ok").trim() || "#34d07f";
  return { follow, followFill: "rgba(52,208,127,0.12)", onFollow: "#06140c" };
}

/**
 * `camera()` answers the camera to draw, or null. `picture` says whether to
 * draw its picture or only the boxes. `onGone` runs once the canvas has left
 * the page.
 */
export function livePicture(hub, camera, act, { picture = true, onGone } = {}) {
  const canvas = el("canvas.cam-live", { width: 640, height: 360 });
  let pics = null;
  let client = null;
  let raf = 0;
  let last = 0;
  let tint = null;
  let born = false;

  canvas.addEventListener("click", (e) => {
    const cam = camera();
    if (!cam) return;
    const r = canvas.getBoundingClientRect();
    const who = personAt(hub.seen.get(cam.id), (e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height);
    if (!who) return;
    e.stopPropagation();
    act(() => hub.api.followPerson(cam.id, who));
  });
  canvas.addEventListener("dblclick", (e) => {
    // A double click on a person is two taps on them, not a take.
    const cam = camera();
    const r = canvas.getBoundingClientRect();
    if (cam && personAt(hub.seen.get(cam.id), (e.clientX - r.left) / r.width, (e.clientY - r.top) / r.height)) e.stopPropagation();
  });

  function paint(t) {
    raf = requestAnimationFrame(paint);
    if (canvas.isConnected) born = true;
    else if (born) return stop(true);
    if (t - last < FRAME_MS || !canvas.isConnected) return;
    last = t;
    const cam = camera();
    const ctx = canvas.getContext("2d");
    const seen = cam && hub.seen.get(cam.id);
    canvas.style.cursor = seen && seen.people.length ? "pointer" : "";
    if (!cam) return ctx.clearRect(0, 0, canvas.width, canvas.height);
    const from = hub.demo || hub.real;
    if (from !== client) (client = from), (pics = pictures(from));
    if (picture || hub.demo) pics.draw(cam, canvas);
    else ctx.clearRect(0, 0, canvas.width, canvas.height);
    tint = tint || colours(canvas);
    drawPeople(ctx, canvas.width, canvas.height, seen, tint);
  }

  function stop(gone) {
    cancelAnimationFrame(raf);
    raf = 0;
    if (gone && onGone) onGone();
  }

  return {
    canvas,
    start() {
      if (!raf) raf = requestAnimationFrame(paint);
    },
    stop: () => stop(false),
    retheme: () => (tint = null),
  };
}
