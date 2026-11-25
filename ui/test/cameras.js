// PTZ cameras and ATEM switchers where they sit in GodwinMix, against the
// demo backend (panels/camera/demo/): the adapter's tool names and shapes,
// the joystick's rate and its stops, the pretend follow loop, and then the
// real parts driven by clicks and pointer events: the camera button on a
// source tile, camera properties in the side drawer, the overlay on Preview,
// the switcher chips, switcher properties, the Outputs rows and the
// Volunteer layout. No core and no hardware.

import { wait, until } from "./wall-help.js";
import { cameraPartsTests } from "./cameras-parts.js";

export async function cameraTests(test, eq, ok) {
  const { TOOLS, unwrap, cameraApi } = await import("../panels/camera/api.js");
  const { demoClient } = await import("../panels/camera/demo/client.js");
  const { Drive, RATE } = await import("../panels/camera/drive.js");
  const { trackWords, badgeWords } = await import("../panels/camera/words.js");

  test("a tool's answer is read from structured_content, as the core passes the plugin's on", () => {
    eq(unwrap({ content: [{ type: "text", text: "two cameras." }], structured_content: { cameras: [] } }), { cameras: [] });
    eq(unwrap({ structuredContent: { a: 1 } }), { a: 1 });
    eq(unwrap({ content: [{ type: "text", text: '{"b":2}' }] }), { b: 2 });
    let said = "";
    try {
      unwrap({ content: [{ type: "text", text: "cam-x has no source." }], is_error: true });
    } catch (e) {
      said = e.message;
    }
    eq(said, "cam-x has no source.", "a refusal flagged is_error throws its sentence");
  });

  const { tagSpot } = await import("../panels/camera/picture.js");
  test("two people standing close keep name tags that do not cover each other", () => {
    const placed = [];
    eq(tagSpot(placed, 100, 10, 80, 24, 360), [100, 10]);
    eq(tagSpot(placed, 130, 10, 80, 24, 360), [130, 36], "the second tag moves down below the first");
    eq(tagSpot(placed, 300, 10, 80, 24, 360), [300, 10], "a tag clear of the others stays where it is");
  });

  const { camera, people, tracking, atem: atemShape } = await import("../panels/camera/shapes.js");
  test("people the tracker has lost, or who left the picture, are not drawn", () => {
    const seen = people({ camera: "cam-b", tracks: [
      { id: "person-1", box: { x: 0.2, y: 0.1, w: 0.2, h: 0.8 }, state: "confirmed" },
      { id: "person-2", box: { x: 0.5, y: 0.1, w: 0.2, h: 0.8 }, state: "lost" },
      { id: "person-3", box: { x: -0.3, y: 0.1, w: 0.31, h: 0.8 }, state: "confirmed" },
      { id: "person-4", box: { x: 0.9, y: 0.1, w: 0.3, h: 0.8 }, state: "confirmed" },
    ] });
    eq(seen.people.map((p) => p.id), ["person-1", "person-4"]);
    const watching = people({ camera: "cam-b", mode: "observing", target: "person-1", tracks: [{ id: "person-1", box: { x: 0.2, y: 0.1, w: 0.2, h: 0.8 }, state: "confirmed", followed: true }] });
    eq([watching.following, watching.people[0].followed], [null, false], "only a steering camera follows anyone");
    ok(Math.abs(seen.people[1].box[2] - 0.1) < 1e-9, "a box half out of the picture is cut at its edge");
  });
  test("the atem plugin's state becomes the drawer's", () => {
    const w = atemShape({
      id: "atem-main", name: "Main switcher", connected: true, why: null, mode: "follow", product: "ATEM Mini Pro", firmware: "8.1.1",
      inputs: [{ id: 1, long: "Camera 1", short: "CAM1" }, { id: 2, long: "", short: "CAM2" }],
      mes: [{ me: 0, program: 1, preview: 2, transition: { style: "dip", rate: 30, position: 0.5, in_transition: true }, usk: [{ keyer: 0, on_air: true }], ftb: { on: false, rate: 25 } }],
      dsk: [{ keyer: 0, on_air: true, tie: true, rate: 25 }], aux: [{ aux: 0, input: 10010 }],
      macros: [{ index: 3, name: "Closing", description: "" }], macro: { running: false, index: null, last_run: 3 },
      tally: { program: [1], preview: [2], sources: { "cam-a": "program" } }, map: { "cam-a": 1 },
    });
    eq(w.inputs, [{ id: 1, name: "Camera 1", short: "CAM1" }, { id: 2, name: "CAM2", short: "CAM2" }]);
    eq(w.mes[0], { program: 1, preview: 2, style: "dip", rate: 30, position: 0.5, running: 1 });
    eq([w.dsks[0].on, w.dsks[0].tie, w.usks[0].on, w.ftb.on, w.auxes[0].source], [true, true, true, false, 10010]);
    eq([w.lastMacro, w.model, w.map["cam-a"], w.name], [3, "ATEM Mini Pro", 1, "Main switcher"]);
  });
  test("the ptz plugin's shapes become the drawer's, and the tile's badge speaks only while it steers", () => {
    eq(camera({ id: "cam-pulpit", protocol: "visca-ip", source: "pulpit", last_error: null, tracking: true }).name, "Pulpit");
    const s = { camera: "cam-a", mode: "following", state: "holding", framing: "full", target: "person-3",
      tracks: [{ id: "person-3", box: { x: 0.1, y: 0.2, w: 0.3, h: 0.4 }, confidence: 0.9, state: "confirmed", followed: true }], error: null };
    eq(people(s).people[0].box, [0.1, 0.2, 0.3, 0.4]);
    eq(people(s).following, "person-3");
    eq(trackWords(tracking(s)), "Lost them, holding the shot");
    eq(trackWords(tracking({ camera: "cam-a", mode: "stopped", tracks: [] })), "Tracking off");
    eq(trackWords(tracking({ ...s, state: "following" })), "Following: full length");
    eq([badgeWords(tracking({ ...s, state: "following" })), badgeWords(tracking({ ...s, mode: "observing" }))], ["Following", ""]);
  });

  const demo = demoClient({ clock: false });
  const api = cameraApi(demo);
  const cams = await api.cameras();
  test("every camera call goes through tool.call under the name in TOOLS", () => {
    eq(cams.map((c) => c.id), ["cam-wide", "cam-centre", "cam-side"]);
    ok(demo.calls.every((c) => c.method === "tool.call" && Object.values(TOOLS).includes(c.params.name)), "only names from the table");
    ok(Object.values(TOOLS).filter((n) => !n.startsWith("atem/")).every((n) => n.startsWith("ptz/")), "every camera tool is the ptz plugin's");
  });
  const standIn = demoClient({ clock: false, sources: [{ id: "pulpit", name: "Pulpit" }, { id: "wide", name: "Wide" }] });
  const stood = await cameraApi(standIn).cameras();
  test("the demo stands in for the sources it is given, the first as the centre camera", () => {
    eq(stood.map((c) => [c.id, c.source, c.name]), [["cam-centre", "pulpit", "Pulpit"], ["cam-wide", "wide", "Wide"]]);
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
  const x0 = demo.demo.cams[0].view.cx;
  await api.move("cam-wide", 1, 0, 1);
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
  const drive = new Drive({ move: (c, p, t, z) => sent.push(["move", p, t, z]), still: () => sent.push(["stop"]) });
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
  test("the joystick sends at most fifteen moves a second, and zero speeds when let go", () => {
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

  await demo.call("program.take", { source: "cam-side" });
  const after = await api.atemState("atem-main");
  test("in follow mode a GodwinMix take moves the switcher's programme too", () => {
    eq(after.mes[0].program, 3);
  });

  await cameraPartsTests(test, eq, ok);
}
