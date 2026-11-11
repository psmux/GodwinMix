// The camera window against its demo backend (panels/cameras/demo/): the
// adapter's tool names, the joystick's rate and its stops, where a take goes
// for each switcher mode, the tally, the pretend follow loop, and the panel
// itself driven by clicks and pointer events. No core and no hardware.

import { wait, until } from "./wall-help.js";

export async function cameraTests(test, eq, ok) {
  const { TOOLS, unwrap, cameraApi } = await import("../panels/cameras/api.js");
  const { demoClient } = await import("../panels/cameras/demo/client.js");
  const { Drive, RATE } = await import("../panels/cameras/drive.js");
  const { route, tallyOf, trackWords } = await import("../panels/cameras/words.js");
  const { plan, LOWER_THIRD } = await import("../panels/cameras/take.js");

  test("a tool's answer is read as MCP structured content, as JSON text, or as it came", () => {
    eq(unwrap({ structuredContent: { a: 1 } }), { a: 1 });
    eq(unwrap({ content: [{ type: "text", text: '{"b":2}' }] }), { b: 2 });
    eq(unwrap({ c: 3 }), { c: 3 });
  });

  const demo = demoClient({ clock: false });
  const api = cameraApi(demo);
  const cams = await api.cameras();
  test("every camera call goes through tool.call under the name in TOOLS", () => {
    eq(cams.map((c) => c.id), ["cam-wide", "cam-centre", "cam-side"]);
    ok(demo.calls.every((c) => c.method === "tool.call" && Object.values(TOOLS).includes(c.params.name)), "only names from the table");
  });

  await api.followPerson("cam-centre", "person-1");
  await api.framing("cam-centre", "close");
  const before = demo.demo.cams[1].view.w;
  for (let i = 0; i < 40; i++) demo.step(0.05);
  const seen = await api.observe("cam-centre");
  const state = await api.trackState("cam-centre");
  test("following a person moves the camera in on them and the tracker says so", () => {
    eq(seen.following, "person-1");
    ok(seen.people.some((p) => p.id === "person-1"), "they are in the picture");
    ok(demo.demo.cams[1].view.w < before, "it zoomed in for a close shot");
    eq(trackWords(state), "Following: close up");
  });

  await api.trackStop("cam-wide");
  await api.move("cam-wide", 1, 0);
  const x0 = demo.demo.cams[0].view.cx;
  await api.zoom("cam-wide", 1);
  for (let i = 0; i < 4; i++) demo.step(0.05);
  const x1 = demo.demo.cams[0].view.cx;
  for (let i = 0; i < 20; i++) demo.step(0.05);
  test("a velocity moves a camera and stops by itself 300 ms after the last one", () => {
    ok(x1 > x0, "it panned right");
    const x2 = demo.demo.cams[0].view.cx;
    demo.step(0.5);
    eq(demo.demo.cams[0].view.cx, x2, "and then stood still");
  });

  const sent = [];
  const drive = new Drive({ move: (c, p, t) => sent.push(["move", p, t]), stop: () => sent.push(["stop"]), zoom: (c, s) => sent.push(["zoom", s]) });
  drive.use("cam-wide");
  const t0 = performance.now();
  while (performance.now() - t0 < 600) {
    drive.move(Math.random(), 0.5);
    await wait(5);
  }
  await wait(30);
  const moves = sent.filter((s) => s[0] === "move").length;
  drive.stopAll();
  await wait(10);
  test("the joystick sends at most fifteen moves a second, and a stop when let go", () => {
    ok(moves >= 5 && moves <= Math.ceil(0.65 * RATE) + 1, `${moves} moves in 600 ms`);
    eq(sent[sent.length - 1], ["stop"]);
  });
  sent.length = 0;
  await wait(100);
  drive.move(0.8, 0);
  await wait(10);
  window.dispatchEvent(new Event("blur"));
  await wait(10);
  test("the camera stops when the window loses focus", () => eq(sent.map((s) => s[0]), ["move", "stop"]));
  sent.length = 0;
  drive.move(0.8, 0);
  await wait(400);
  test("a held stick is sent again before the camera's 300 ms runs out", () => {
    ok(sent.length >= 2, `${sent.length} sends in 400 ms`);
  });
  drive.destroy();

  const atem = await api.atemState("atem-main");
  test("a take goes where the switcher's mode says", () => {
    eq(route(null).words, "GodwinMix");
    eq([route(atem).mix, route(atem).atem], [true, false], "follow: GodwinMix takes and the ATEM follows");
    eq([route({ ...atem, mode: "lead" }).mix, route({ ...atem, mode: "lead" }).atem], [false, true]);
    eq([route({ ...atem, mode: "own" }, "both").mix, route({ ...atem, mode: "own" }, "both").atem], [true, true]);
  });

  test("on air and next come from GodwinMix's tally or the ATEM's buses", () => {
    const cam = { id: "cam-side", source: "cam-side" };
    eq(tallyOf(cam, { tally: { "cam-side": "program" } }, null, null), "program");
    eq(tallyOf(cam, {}, { ...atem, mes: [{ program: 3, preview: 1 }] }, null), "program");
    eq(tallyOf(cam, {}, { ...atem, mes: [{ program: 1, preview: 3 }] }, null), "preview");
    eq(tallyOf(cam, {}, null, "cam-side"), "preview");
  });

  const log = [];
  const fake = new Proxy({}, { get: (_, k) => (...a) => log.push([k, ...a]) });
  const centre = { id: "cam-centre", source: "cam-centre" };
  const own = { ...atem, mode: "own" };
  for (const s of plan("fade", { api: fake, cam: centre, atem: own, pick: "both" })) s();
  for (const s of plan("lower", { api: fake, cam: centre, atem: null, lower: false })) s();
  test("Fade to both cuts GodwinMix with a fade and runs the ATEM's auto on the mapped input", () => {
    eq(log, [["take", "cam-centre", "fade"], ["atemPreview", "atem-main", 2], ["atemAuto", "atem-main"], ["key", LOWER_THIRD, true]]);
  });

  await demo.call("program.take", { source: "cam-side" });
  const after = await api.atemState("atem-main");
  test("in follow mode a GodwinMix take moves the ATEM's programme too", () => {
    eq(after.mes[0].program, 3);
  });

  await panelTests(test, eq, ok);
}

async function panelTests(test, eq, ok) {
  try {
    localStorage.setItem("gmx.cameras", JSON.stringify({ demo: true }));
  } catch { /* fine */ }
  const { mount } = await import("../panels/cameras/mount.js");
  const host = document.createElement("div");
  host.style.cssText = "position:fixed;left:-3000px;top:0;width:1200px";
  document.body.append(host);
  host.client = { call: async () => { throw new Error("offline"); }, on: () => () => {}, onRender: () => () => {}, listen: () => () => {} };
  const view = mount(host);
  await until(() => view.client && view.client.demo && host.querySelectorAll(".cam-card").length === 3, 4000);
  const client = view.client;
  test("the window shows the demo's three cameras and its switcher bar", () => {
    eq(host.querySelectorAll(".cam-card").length, 3);
    ok(!host.querySelector(".cam-atem").hidden, "the ATEM bar is up");
  });

  host.querySelector('.cam-card[data-cam="cam-side"]').click();
  await wait(60);
  test("tapping a card makes it next, with a green frame", () => {
    eq(view.model.next, "cam-side");
    ok(host.querySelector('.cam-card[data-cam="cam-side"]').classList.contains("preview"));
  });

  client.calls.length = 0;
  host.querySelector(".cam-take.cut").click();
  await until(() => client.calls.some((c) => c.method === "program.take"));
  await wait(200);
  test("Cut puts the next camera on air and its frame turns red", () => {
    eq(client.calls.find((c) => c.method === "program.take").params, { source: "cam-side" });
    ok(host.querySelector('.cam-card[data-cam="cam-side"]').classList.contains("program"));
  });

  const stick = host.querySelector(".cam-stick");
  const r = stick.getBoundingClientRect();
  client.calls.length = 0;
  stick.dispatchEvent(new PointerEvent("pointerdown", { pointerId: 7, clientX: r.right - 10, clientY: r.top + r.height / 2, bubbles: true }));
  await wait(200);
  stick.dispatchEvent(new PointerEvent("pointerup", { pointerId: 7, bubbles: true }));
  await wait(50);
  const names = client.calls.filter((c) => c.method === "tool.call").map((c) => c.params.name);
  test("dragging the joystick sends moves to the chosen camera, and letting go sends a stop", () => {
    ok(names.includes("ptz/move"), names.join());
    eq(names.filter((n) => n === "ptz/move" || n === "ptz/stop").pop(), "ptz/stop");
    eq(client.calls.find((c) => c.params.name === "ptz/move").params.arguments.camera, "cam-side");
  });

  view.destroy();
  host.remove();
  try {
    localStorage.removeItem("gmx.cameras");
  } catch { /* fine */ }
}
