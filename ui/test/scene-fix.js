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
