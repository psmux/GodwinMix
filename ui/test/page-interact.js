// Interact: where a pointer on the picture lands on the page, which keys go
// to it, and what the view sends, against a stub client that records calls.

import { contentBox, toShare, pageScale, wheelPixels, keyFor, clickCount, coalesce } from "../panels/sources/page-geometry.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

export async function pageInteractTests(test, eq, ok) {
  test("page: a 16:9 picture in a square box has bars above and below", () => {
    eq(contentBox(400, 400, 1280, 720), { x: 0, y: 87.5, w: 400, h: 225 });
    eq(contentBox(800, 300, 1280, 720), { x: 133.33333333333331, y: 0, w: 533.3333333333334, h: 300 });
    eq(contentBox(0, 0, 1280, 720), { x: 0, y: 0, w: 0, h: 0 });
  });

  test("page: points are shares of the picture, not of the box around it", () => {
    eq(toShare(200, 200, 400, 400, 1280, 720), { x: 0.5, y: 0.5, inside: true });
    eq(toShare(0, 87.5, 400, 400, 1280, 720), { x: 0, y: 0, inside: true });
    // On the bar above the picture: pinned to its top edge, and said to be off it.
    eq(toShare(100, 10, 400, 400, 1280, 720), { x: 0.25, y: 0, inside: false });
    // No picture yet: the box stands in for it.
    eq(toShare(30, 15, 120, 60, 0, 0), { x: 0.25, y: 0.25, inside: true });
  });

  test("page: a finger's drag is scaled to the page's own pixels", () => {
    eq(pageScale(1920, 640), 3);
    eq(pageScale(0, 640), 1);
    eq(wheelPixels(0, 3, 1), { dx: 0, dy: 120 });
    eq(wheelPixels(0, 100.4, 0), { dx: 0, dy: 100 });
  });

  test("page: printable keys carry their character, named keys their code, the rest stay here", () => {
    eq(keyFor({ key: "a", keyCode: 65 }), { type: "key", code: 65, key: "a" });
    eq(keyFor({ key: "Enter", keyCode: 13 }), { type: "key", code: 13 });
    eq(keyFor({ key: "a", keyCode: 65, ctrlKey: true }), { type: "key", code: 65, key: "a", ctrl: true });
    eq(keyFor({ key: "Shift", keyCode: 16, shiftKey: true }), null);
    eq(keyFor({ key: "Process", keyCode: 229 }), null);
    eq(keyFor({ key: "k", keyCode: 75, isComposing: true }), null);
  });

  test("page: a second press close by is a double click", () => {
    const first = { x: 10, y: 10, t: 0 };
    eq(clickCount(null, first), 1);
    eq(clickCount({ ...first, count: 1 }, { x: 12, y: 11, t: 200 }), 2);
    eq(clickCount({ ...first, count: 1 }, { x: 40, y: 11, t: 200 }), 1);
    eq(clickCount({ ...first, count: 1 }, { x: 10, y: 10, t: 900 }), 1);
  });

  test("page: moves waiting behind a call are kept as the newest", () => {
    const q = [];
    coalesce(q, { type: "move", x: 0.1, y: 0.1 });
    coalesce(q, { type: "move", x: 0.2, y: 0.2 });
    coalesce(q, { type: "down", x: 0.2, y: 0.2 });
    coalesce(q, { type: "move", x: 0.3, y: 0.3 });
    eq(q.map((e) => `${e.type} ${e.x}`), ["move 0.2", "down 0.2", "move 0.3"]);
  });

  // The view itself, against a client that records what it is asked.
  const calls = [];
  const client = {
    transport: {},
    call: async (method, params) => {
      calls.push([method, JSON.parse(JSON.stringify(params))]);
      return method === "source.page.navigate" ? { url: params.url } : { sent: 1, dropped: 0 };
    },
    onRender: () => () => {},
    store: { source: () => ({ id: "site", page: { url: "https://example.com/", can_back: true } }) },
  };
  const { openInteract } = await import("../panels/sources/page-interact.js");
  const dialog = openInteract(client, { id: "site", name: "Site" });
  try {
    const img = dialog.el.querySelector(".page-picture");
    const surface = dialog.el.querySelector(".page-surface");
    // Sized here: the view's stylesheet may not have arrived yet.
    Object.assign(img.style, { width: "320px", height: "180px", display: "block" });
    img.removeAttribute("src");
    await tick(20);
    const r = img.getBoundingClientRect();
    const at = { clientX: r.left + r.width / 2, clientY: r.top + r.height / 4, pointerId: 3, pointerType: "mouse", bubbles: true, cancelable: true };
    surface.dispatchEvent(new PointerEvent("pointerdown", { ...at, button: 0 }));
    surface.dispatchEvent(new PointerEvent("pointerup", { ...at, button: 0 }));
    await tick(20);
    const sent = calls.filter(([m]) => m === "source.page.input").flatMap(([, p]) => p.events);
    test("page: a click on the picture is a move, a down and an up where it landed", () => {
      eq(sent.map((e) => e.type), ["move", "down", "up"]);
      eq([sent[1].x, sent[1].y, sent[1].button, sent[1].count], [0.5, 0.25, "left", 1]);
      ok(calls.every(([, p]) => p.id === "site"), "every call names the source");
    });

    const url = dialog.el.querySelector(".page-url");
    url.value = "example.org/news";
    dialog.el.querySelector(".page-bar").requestSubmit();
    await tick(20);
    test("page: the address bar goes there through source.page.navigate", () => {
      const nav = calls.find(([m]) => m === "source.page.navigate");
      ok(nav, "a navigate call");
      eq(nav[1], { id: "site", action: "go", url: "example.org/news" });
    });
  } finally {
    dialog.close();
  }
}
