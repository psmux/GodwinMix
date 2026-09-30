// A scene's sources that are not running: the check, the note, the mark and
// the Fix dialog, against a fake client that answers as the core does.

function fakeClient(sources, missing) {
  const calls = [];
  const store = { source: (id) => sources.find((s) => s.id === id) || null };
  return {
    calls,
    store,
    state: { sources },
    onRender: () => () => {},
    call: async (method, params) => {
      calls.push({ method, params });
      if (method === "source.missing") return missing.filter((m) => params.ids.includes(m.id));
      return { ok: true };
    },
  };
}

function fakeScenes(client, records) {
  const groups = [];
  return {
    groups,
    summary: (id) => ({ id, name: "Default", sources: [...new Set(records.map((r) => r.content.source))] }),
    scenes: () => [{ id: "default", name: "Default" }],
    mirror: { descendants: () => records },
    call: (method, params) => client.call(method, params),
    reread: async () => {},
    undo: { group: async (label, fn, opts) => { groups.push({ label, opts }); return fn(); } },
  };
}

export async function sceneFixTests(test, eq, ok) {
  const health = await import("../shell/scene-health.js");
  const note = await import("../panels/scenes/fix-note.js");
  const sources = [{ id: "cam-wide", name: "Wide", state: "live" }, { id: "cam1", name: "Camera 1", state: "failed" }];
  const records = ["cam-wide", "cam1", "lyrics", "slides"].map((source, i) => ({ id: `item-${i}`, kind: "item", content: { source } }));
  const missing = [
    { id: "cam1", name: "Camera 1", why: "failed", restore: false },
    { id: "lyrics", name: "Lyrics", why: "not_started", error: "no browser sidecar", restore: true },
    { id: "slides", name: "Slides", why: "removed", restore: true },
  ];
  const client = fakeClient(sources, missing);

  test("a failed source and one the mixer does not have are both not running", () => {
    eq(health.notRunning(client, ["cam-wide", "cam1", "lyrics"]), ["cam1", "lyrics"]);
  });
  test("before the first status nothing is called missing", () => {
    eq(health.notRunning(fakeClient([], []), ["cam1"]), []);
  });

  const box = document.createElement("div");
  const ids = health.notRunning(client, note.sourcesOf(records));
  note.fillNote(box, client, ids, () => ({}), "The take goes ahead without them.");
  const button = box.querySelector("button");
  note.fillNote(box, client, ids, () => ({}), "The take goes ahead without them.");
  test("the note counts them and keeps its Fix button across redraws", () => {
    ok(box.textContent.startsWith("3 sources here are not running"), box.textContent);
    ok(button && box.querySelector("button") === button, "the button was swapped out under a click");
  });

  const scenes = fakeScenes(client, records);
  const { openFix } = await import("../panels/scenes/fix.js");
  const dialog = await openFix({ client, scenes, scene: "default" });
  const rows = [...dialog.el.querySelectorAll(".fix-row")];
  test("Fix lists each one with why and the thing that brings it back", () => {
    eq(rows.length, 3);
    ok(rows[0].textContent.includes("Retry"), rows[0].textContent);
    ok(rows[1].textContent.includes("no browser sidecar") && rows[1].textContent.includes("Try again"), rows[1].textContent);
    ok(rows[2].textContent.includes("Put back"), rows[2].textContent);
    ok(rows.every((r) => r.querySelector("input").checked), "not every box starts checked");
  });
  rows[2].querySelector("input").click();
  dialog.el.querySelector("footer .btn.primary").click();
  await new Promise((r) => setTimeout(r, 50));
  const removed = client.calls.filter((c) => c.method === "scene.item.remove").map((c) => c.params.item);
  test("Remove takes the checked ones out as one undo step, and leaves the unchecked", () => {
    eq(removed, ["item-1", "item-2"]);
    eq(scenes.groups.length, 1);
    ok(scenes.groups[0].opts.offer, "the removal offers Undo");
  });
}

/**
 * The cases a real browser found: a mirror that has not read the scene yet,
 * a source nothing can bring back, and the mark's press reaching the pointer
 * pipelines above it. Then the scene list read once the socket opens.
 */
export async function sceneFixLoadTests(test, eq, ok) {
  const sources = [{ id: "cam-wide", name: "Wide", state: "live" }];
  const records = ["cam-wide", "lyrics", "slides"].map((source, i) => ({ id: `item-${i}`, kind: "item", name: source === "lyrics" ? "Lyrics" : null, content: { source } }));
  const client = fakeClient(sources, [{ id: "slides", why: "unknown", restore: false }]);
  const scenes = fakeScenes(client, records);
  let read = false;
  scenes.mirror = { descendants: () => (read ? records : []) };
  scenes.reread = async () => { read = true; };
  const { openFix } = await import("../panels/scenes/fix.js");
  const dialog = await openFix({ client, scenes, scene: "default" });
  const rows = [...dialog.el.querySelectorAll(".fix-row")];
  test("Fix reads a scene the mirror does not have yet, and lists what it draws", () => {
    ok(read, "the scene was never read");
    eq(rows.length, 2);
  });
  test("every row has a name, a reason and a button of its own", () => {
    ok(rows[0].querySelector("strong").textContent === "Lyrics", rows[0].textContent);
    ok(rows.every((r) => r.querySelector(".fix-text > div.sm").textContent.length > 10), "a row has no reason");
    ok(rows.every((r) => r.querySelector(".fix-actions button")), "a row has nothing to press");
  });
  rows[1].querySelector(".fix-actions button").click();
  await new Promise((r) => setTimeout(r, 50));
  test("a row's own Remove takes that one source out, as one undo step", () => {
    eq(client.calls.filter((c) => c.method === "scene.item.remove").map((c) => c.params.item), ["item-2"]);
    ok(scenes.groups.length === 1 && scenes.groups[0].opts.offer, "no Undo offered");
  });
  dialog.close();

  const marks = await import("../panels/scenes/marks.js");
  const strip = document.createElement("div");
  const holder = document.createElement("span");
  const tab = document.createElement("button");
  holder.append(tab);
  strip.append(holder);
  document.body.append(strip);
  let pressed = 0;
  strip.addEventListener("pointerdown", () => pressed++);
  const panel = { client, scenes, tabs: new Map([["default", tab]]), tiles: new Map() };
  marks.paint(panel);
  const mark = holder.querySelector(".scene-warn");
  mark.dispatchEvent(new PointerEvent("pointerdown", { bubbles: true, isPrimary: true, button: 0 }));
  test("a press on the mark stays on the mark", () => {
    ok(mark && !mark.hidden, "no mark painted");
    eq(pressed, 0);
  });
  strip.remove();

  const { SceneClient } = await import("../kits/protocol/index.js");
  const listeners = new Map();
  let open = false;
  let lists = 0;
  const socket = {
    state: {},
    on: (name, fn) => { listeners.set(name, fn); return () => listeners.delete(name); },
    opened: () => new Promise((r) => { const was = listeners.get("open"); listeners.set("open", () => { was && was(); r(true); }); }),
    call: async (method) => {
      if (!open) { const e = new Error("not connected"); e.code = -32001; e.data = { retryable: true }; throw e; }
      if (method === "scene.list") { lists++; return { scenes: [{ id: "a", name: "A", items: 2 }] }; }
      if (method === "scene.get") return { id: "a", name: "A", records: [] };
      return {};
    },
  };
  const sc = new SceneClient(socket);
  const started = sc.start();
  await new Promise((r) => setTimeout(r, 20));
  test("no scene list is drawn as empty before the socket opens", () => eq(lists, 0));
  open = true;
  listeners.get("open")();
  await started;
  test("the scene list is read once the socket opens", () => {
    eq(sc.scenes().map((s) => s.name), ["A"]);
    eq(lists, 1);
  });
  open = false;
  await sc.refresh();
  test("a dropped socket keeps the scenes drawn", () => eq(sc.scenes().length, 1));
  open = true;
  listeners.get("open")();
  await new Promise((r) => setTimeout(r, 20));
  test("a reconnect reads the scenes again", () => eq(lists, 2));
  sc.stop();
}
