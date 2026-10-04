// New scene from the green screen layout, and the key editor, against a fake
// client that
// answers as the core does.

function fakeClient() {
  const calls = [];
  return {
    calls,
    state: { sources: [{ id: "cam", name: "Studio camera" }, { id: "logo", name: "logo" }] },
    snapshotUrl: (id, w) => `/api/v1/snapshot/${id}?width=${w}`,
    call: async (method, params) => {
      calls.push({ method, params });
      if (method === "media.list") return { items: [{ name: "newsroom.png" }, { name: "desk.png" }] };
      if (method === "source.key_color") return { color: params.x === undefined ? "#2fb04c" : "#30b050", found: "green" };
      if (method === "scene.create_from") return { id: "s1", name: "Presenter", key: "#2fb04c", key_from: "guessed", added: ["newsroom"] };
      return {};
    },
  };
}

const tick = () => new Promise((r) => setTimeout(r, 30));

export async function greenScreenTests(test, eq, ok) {
  const vs = await import("../panels/scenes/green-screen.js");
  test("the pickers offer library files first, then sources, and a way to have nothing in front", () => {
    const got = vs.choices([{ name: "newsroom.png" }], [{ id: "cam", name: "Studio camera" }], { none: "Nothing in front" });
    eq(got.map((c) => c.value), ["", "newsroom.png", "cam"]);
    eq(got[2].label, "Studio camera (cam)");
  });
  test("the request is the layout's slots in order, leaving out what was not picked", () => {
    eq(vs.request({ background: "newsroom.png", presenter: "cam", foreground: "", name: " " }), { sources: ["newsroom.png", "cam"], layout: "virtual-set", name: "Presenter" });
    eq(vs.request({ background: "a", presenter: "cam", foreground: "desk.png", name: "News" }), { sources: ["a", "cam", "desk.png"], layout: "virtual-set", name: "News" });
  });

  const client = fakeClient();
  let made = null;
  let refreshed = false;
  const dialog = await vs.openGreenScreen({ client, scenes: { refresh: async () => { refreshed = true; } }, onMade: (a) => { made = a; } });
  const selects = [...dialog.el.querySelectorAll("select")];
  selects[0].value = "newsroom.png";
  selects[1].value = "cam";
  selects[2].value = "desk.png";
  dialog.el.querySelector("footer .btn.primary").click();
  await tick();
  test("Make the scene makes one scene.create_from call with what was picked", () => {
    const call = client.calls.find((c) => c.method === "scene.create_from");
    ok(call, "no scene.create_from call");
    eq(call.params, { sources: ["newsroom.png", "cam", "desk.png"], layout: "virtual-set", name: "Presenter" });
    ok(made && made.id === "s1", "the panel was not told about the new scene");
    ok(refreshed, "the scene list was not read again");
    ok(!document.body.contains(dialog.el), "the dialog stayed open");
  });

  const { keyEditor, pointOf, withParam } = await import("../panels/composer/key.js");
  test("a click on the still is a point from 0 to 1 across and down", () => {
    eq(pointOf({ clientX: 150, clientY: 75 }, { left: 100, top: 50, width: 200, height: 100 }), { x: 0.25, y: 0.25 });
    eq(pointOf({ clientX: 0, clientY: 900 }, { left: 100, top: 50, width: 200, height: 100 }), { x: 0, y: 1 });
  });
  test("changing one setting keeps the others", () => {
    eq(withParam({ color: "#30b050", spill: 0.5 }, "spill", 0.8), { color: "#30b050", spill: 0.8 });
  });

  const sets = [];
  const editor = keyEditor({ client, filter: { type: "chroma/filter", params: { color: "auto", spill: 0.5 } }, ref: "Key", source: "cam", set: async (ref, params) => sets.push({ ref, params }) });
  document.body.appendChild(editor);
  [...editor.querySelectorAll("button")].find((b) => b.textContent === "Find").click();
  await tick();
  test("Find asks the core for the screen colour and puts it on the key", () => {
    ok(client.calls.some((c) => c.method === "source.key_color" && c.params.id === "cam" && c.params.x === undefined), "source.key_color was not asked");
    eq(sets.at(-1), { ref: "Key", params: { color: "#2fb04c", spill: 0.5 } });
  });
  const spill = editor.querySelector('input[aria-label="Spill"]');
  spill.value = "0.9";
  spill.dispatchEvent(new Event("input"));
  await new Promise((r) => setTimeout(r, 200));
  test("a slider sends the whole key with its one value changed", () => {
    eq(sets.at(-1).params, { color: "#2fb04c", spill: 0.9 });
    ok(editor.querySelector('input[aria-label="Matte left"]'), "no garbage matte control");
  });
  editor.remove();
}
