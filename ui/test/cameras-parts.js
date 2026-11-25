// The camera and switcher parts in place, against the demo: a mixer with
// no ptz or atem plugin, Camera on a tile offering simulated cameras, the
// camera button and badge on the tiles, camera properties in the drawer, the
// overlay on Preview, the Programme header's chips, switcher properties, the
// Outputs rows and the Volunteer layout.

import { wait, until, button } from "./wall-help.js";

/** A mixer with three sources and no camera plugins, as a page sees it. */
function mixer() {
  const renders = new Set();
  const state = { program: "wide", sources: [{ id: "pulpit", name: "Pulpit" }, { id: "wide", name: "Wide" }, { id: "slides", name: "Slides" }, { id: "notes", name: "Notes" }] };
  const client = {
    state,
    calls: [],
    async call(method, params) {
      client.calls.push({ method, params });
      if (method === "plugin.list") return { plugins: [] };
      throw new Error(`${method}: no mixer is answering`);
    },
    on: () => () => {},
    onRender(fn) {
      renders.add(fn);
      fn(state);
      return () => renders.delete(fn);
    },
    take(id) {
      state.program = id;
      for (const fn of renders) fn(state);
    },
    store: { tallyOf: (id) => (id === state.program ? "program" : "off"), source: (id) => state.sources.find((s) => s.id === id) },
  };
  return client;
}

const names = (hub) => hub.demo.calls.filter((c) => c.method === "tool.call").map((c) => c.params.name);
const lastArgs = (hub, name) => hub.demo.calls.filter((c) => c.params && c.params.name === name).pop()?.params.arguments;

export async function cameraPartsTests(test, eq, ok) {
  const keep = (() => { try { return localStorage.getItem("gmx.cameras.demo"); } catch { return null; } })();
  try { localStorage.removeItem("gmx.cameras.demo"); } catch { /* fine */ }
  // The page's own drawer, as shell.drawer finds it; taken away afterwards
  // so the suites after this one find theirs.
  const slot = document.body.insertBefore(Object.assign(document.createElement("div"), { className: "slot-sidebar" }), document.body.firstChild);
  slot.style.cssText = "position:fixed;left:-2000px;top:0;width:340px";
  const client = mixer();
  const { hubFor } = await import("../panels/camera/hub.js");
  const { openCamera } = await import("../panels/camera/props.js");
  const hub = hubFor(client);

  await openCamera(client, client.state.sources[0]);
  test("with no ptz plugin, Camera says where to get it and offers simulated cameras", () => {
    ok(slot.textContent.includes("needs the ptz plugin"), slot.textContent);
    ok(button(slot, "Try with simulated cameras"), "the demo is offered");
  });

  button(slot, "Try with simulated cameras").click();
  await until(() => slot.querySelector(".cam-props[data-camera]"), 3000);
  test("the simulated cameras stand in for this source, and camera properties opens on it", () => {
    eq(slot.querySelector(".cam-props").dataset.camera, "cam-centre");
    eq(hub.cameraFor("pulpit").id, "cam-centre");
    ok(slot.querySelector("canvas.cam-live"), "the live picture");
    ok(slot.querySelector(".cam-stick") && slot.querySelector(".cam-zoom"), "the joystick and zoom");
    eq([...slot.querySelectorAll(".cam-chip")].map((b) => b.textContent), ["Close", "Waist", "Full", "Wide"]);
    ok([...slot.querySelectorAll("details.cam-more:not([open]) summary")].some((x) => x.textContent === "Exposure and focus"), "exposure and focus folded away");
  });

  const stick = slot.querySelector(".cam-stick");
  const r = stick.getBoundingClientRect();
  hub.demo.calls.length = 0;
  stick.dispatchEvent(new PointerEvent("pointerdown", { pointerId: 7, clientX: r.right - 8, clientY: r.top + r.height / 2, bubbles: true }));
  await wait(200);
  stick.dispatchEvent(new PointerEvent("pointerup", { pointerId: 7, bubbles: true }));
  await wait(60);
  test("the joystick moves this source's camera, and letting go sends zero speeds, never stop", () => {
    const moves = hub.demo.calls.filter((c) => c.params && c.params.name === "ptz/move").map((c) => c.params.arguments);
    ok(moves.length >= 2, JSON.stringify(moves));
    eq(moves[0].camera, "cam-centre");
    eq(moves[moves.length - 1], { camera: "cam-centre", pan: 0, tilt: 0, zoom: 0 });
    ok(!names(hub).includes("ptz/stop"), "the stop tool would turn tracking off");
  });

  await until(() => (hub.seen.get("cam-centre") || {}).people?.length, 3000);
  const who = hub.seen.get("cam-centre").people[0];
  const pic = slot.querySelector("canvas.cam-live");
  const box = pic.getBoundingClientRect();
  pic.dispatchEvent(new MouseEvent("click", { bubbles: true, clientX: box.left + (who.box[0] + who.box[2] / 2) * box.width, clientY: box.top + (who.box[1] + who.box[3] / 2) * box.height }));
  await until(() => lastArgs(hub, "ptz/track_follow"));
  test("tapping a person in the picture follows them", () => eq(lastArgs(hub, "ptz/track_follow"), { camera: "cam-centre", target: who.id }));

  const follows = [...slot.querySelectorAll("details.cam-more")].find((d) => d.querySelector("summary").textContent === "How it follows");
  test("How it follows is folded away under the framing", () => ok(follows && !follows.open));
  follows.open = true;
  await until(() => lastArgs(hub, "ptz/track_tune"));
  await until(() => follows.querySelector('input[aria-label="Speed"]').value === "0.8");
  const slide = (label, value) => {
    const input = follows.querySelector(`input[aria-label="${label}"]`);
    input.value = String(value);
    input.dispatchEvent(new Event("input"));
    input.dispatchEvent(new Event("change"));
  };
  slide("Speed", 0.4);
  await until(() => (lastArgs(hub, "ptz/track_tune") || {}).max_speed === 0.4);
  slide("Smoothness", 10);
  await until(() => (lastArgs(hub, "ptz/track_tune") || {}).max_accel !== undefined);
  test("its sliders tune the camera, and Smoothness to the right is the lowest acceleration", () => {
    eq(hub.demo.demo.cams.find((c) => c.id === "cam-centre").tune.max_speed, 0.4);
    eq(lastArgs(hub, "ptz/track_tune"), { camera: "cam-centre", max_accel: 0.2 });
  });
  const { followTuning } = await import("../panels/camera/follow.js");
  const refused = followTuning(async () => { throw new Error("max_speed must be from 0.05 to 1; 3 changes nothing."); }, () => []);
  refused.root.open = true;
  refused.root.querySelector('input[aria-label="Speed"]').dispatchEvent(new Event("change"));
  await wait(30);
  test("a refusal from the plugin is shown in its own words", () => eq(refused.root.querySelector(".cam-tune-said").textContent, "max_speed must be from 0.05 to 1; 3 changes nothing."));

  const { cameraTiles } = await import("../panels/camera/tiles.js");
  const { buildTile } = await import("../panels/sources/tile.js");
  const tray = { client, tiles: new Map(), isConnected: true, opened: [], openCamera(id) { this.opened.push(id); } };
  for (const s of client.state.sources) tray.tiles.set(s.id, buildTile({ ...s, uri: "test://" }, { audio: { bindFader() {} }, scrub: {}, onMute() {} }));
  const parts = cameraTiles(tray);
  await hub.refresh();
  parts.paint();
  const tile = (id) => tray.tiles.get(id).node;
  test("a source a camera points at gets a camera button beside the gear, and the others none", () => {
    ok(tile("pulpit").querySelector(".gear.camera:not([hidden])"), "the pulpit camera");
    ok(!tile("notes").querySelector(".gear.camera"), "notes has no camera");
    eq(tile("pulpit").querySelector(".gear.camera").nextElementSibling, tile("pulpit").querySelector(".bar > .gear:not(.camera):not(.interact)"));
  });
  await until(() => !tile("pulpit").querySelector(".tile-follow").hidden, 3000);
  test("the tile says Following while its camera steers", () => eq(tile("pulpit").querySelector(".tile-follow").textContent, "Following"));
  tile("pulpit").querySelector(".gear.camera").click();
  test("the camera button opens camera properties for its source", () => eq(tray.opened, ["pulpit"]));
  parts.active(false);

  client.take("pulpit");
  await wait(30);
  const main = await hub.api.atemState("atem-main");
  test("a GodwinMix take reaches the simulated switcher that follows it", () => eq(main.mes[0].program, main.map.pulpit));

  const { cameraIn } = await import("../panels/multiview/studio-camera.js");
  const kit = { summary: (id) => ({ one: { id: "one", sources: ["pulpit", "notes"] }, two: { id: "two", sources: ["pulpit", "wide"] } })[id] };
  test("Preview holds a camera when it holds its source, or a scene with that one camera in it", () => {
    eq(cameraIn(hub, kit, { kind: "source", id: "pulpit" }).id, "cam-centre");
    eq(cameraIn(hub, kit, { kind: "scene", id: "one" }).id, "cam-centre");
    eq(cameraIn(hub, kit, { kind: "scene", id: "two" }), null, "two cameras: no overlay rather than the wrong one");
    eq(cameraIn(hub, kit, { kind: "source", id: "notes" }), null);
  });

  const { el } = await import("../shell/dom.js");
  const previewCanvas = el("canvas", { width: 320, height: 180, style: { width: "480px" } });
  const panel = { previewCanvas, previewWrap: el("div.preview-pane", { style: { position: "fixed", left: "-3000px", width: "480px" } }, [el("div.preview-frame", { style: { position: "relative" } }, [previewCanvas])]) };
  document.body.append(panel.previewWrap);
  const { previewOverlay } = await import("../panels/camera/preview.js");
  const over = previewOverlay(panel, hub);
  over.show(hub.cameraFor("pulpit"));
  await wait(60);
  const ov = panel.previewWrap.querySelector(".cam-ov");
  test("Preview gets the joystick, zoom, framing chips and Track over its picture, and the boxes", () => {
    ok(!ov.hidden, "shown");
    ok(ov.querySelector(".cam-stick") && ov.querySelector(".cam-zoom") && ov.querySelectorAll(".cam-chip").length === 4, "the controls");
    ok(!panel.previewWrap.querySelector("canvas.cam-boxes").hidden, "the boxes canvas");
  });
  ov.querySelector(".cam-ov-toggle").click();
  test("the overlay folds to one camera button", () => ok(ov.classList.contains("folded") && getComputedStyle(ov.querySelector(".cam-ov-body")).display === "none"));
  ov.querySelector(".cam-ov-toggle").click();
  [...ov.querySelectorAll(".cam-chip")].find((b) => b.textContent === "Full").click();
  await until(() => lastArgs(hub, "ptz/track_framing"));
  test("a framing chip on Preview reframes the camera that is following", () => eq(lastArgs(hub, "ptz/track_framing"), { camera: "cam-centre", framing: "full" }));
  over.show(null);
  test("with no camera in Preview the overlay is gone, and so is the reading for it", () => {
    ok(ov.hidden);
    ok(![...hub.wants.keys()].some((k) => k === "people:cam-centre" && hub.wants.get(k) > 1), [...hub.wants.keys()].join());
  });
  over.destroy();
  panel.previewWrap.remove();

  await switcherTests(test, eq, ok, client, hub, slot);
  await hub.setDemo(false);
  test("Leave the demo forgets it", () => ok(!hub.demo && !hub.cams.length));
  try { if (keep) localStorage.setItem("gmx.cameras.demo", keep); } catch { /* fine */ }
  slot.remove();
}

async function switcherTests(test, eq, ok, client, hub, slot) {
  const { el } = await import("../shell/dom.js");
  const programme = { client, isConnected: true, programName: el("span", { text: "Wide" }) };
  const header = el("div.monitor-label", {}, [programme.programName]);
  document.body.append(header);
  const { atemChips } = await import("../panels/camera/atem-chip.js");
  const chips = atemChips(programme);
  await hub.refresh();
  await until(() => header.querySelectorAll(".atem-chip").length === 2, 3000);
  test("the Programme header gets a chip per switcher, with its programme and preview", () => {
    const first = header.querySelector(".atem-chip");
    ok(first.querySelector(".dot.live"), "linked");
    ok(first.textContent.includes("Main switcher"), first.textContent);
    ok(first.querySelector(".atem-bus.pgm").textContent.length > 0 && first.querySelector(".atem-bus.pvw").textContent.length > 0, "both buses");
  });
  header.querySelector(".atem-chip").click();
  await until(() => slot.querySelector(".cam-switcher[data-switcher]"), 3000);
  test("a chip opens switcher properties in the drawer", () => {
    eq(slot.querySelector(".cam-switcher").dataset.switcher, "atem-main");
    eq([...slot.querySelectorAll(".cam-seg[aria-label] .btn")].slice(0, 3).map((b) => b.textContent), ["On its own", "Follows GodwinMix", "Leads GodwinMix"]);
    ok(slot.querySelectorAll(".cam-bus.pgm .cam-key").length >= 4, "the programme bus");
  });
  button(slot, "Leads GodwinMix").click();
  await until(() => lastArgs(hub, "atem/mode"));
  slot.querySelector('.cam-bus.pvw .cam-key[data-input="3"]').click();
  await until(() => lastArgs(hub, "atem/preview"));
  button(slot, "Auto").click();
  await until(() => lastArgs(hub, "atem/auto"));
  test("its mode, preview bus and Auto go to the atem plugin's tools", () => {
    eq(lastArgs(hub, "atem/mode"), { switcher: "atem-main", mode: "lead" });
    eq(lastArgs(hub, "atem/preview"), { switcher: "atem-main", me: 0, input: 3 });
    eq(lastArgs(hub, "atem/auto"), { switcher: "atem-main" });
  });
  const map = slot.querySelector('select[aria-label="Input for Slides"]');
  map.value = "4";
  map.dispatchEvent(new Event("change"));
  await until(() => lastArgs(hub, "atem/map"));
  test("a source is mapped to an input from the drawer", () => eq(lastArgs(hub, "atem/map"), { switcher: "atem-main", source: "slides", input: 4 }));
  chips.active(false);
  header.remove();

  const outputs = { client, isConnected: true, list: el("div.col.pad") };
  const host = el("div", {}, [outputs.list]);
  document.body.append(host);
  const { switcherCards } = await import("../panels/camera/atem-card.js");
  const card = switcherCards(outputs);
  await hub.refresh();
  await until(() => host.querySelectorAll(".atem-row").length === 2, 3000);
  test("Outputs lists the switchers beside the destinations, each with Edit and Remove", () => {
    ok(!outputs.switchers.hidden, "shown with switchers to show");
    ok(button(host, "Add switcher"), "and a way to add one");
    eq([...host.querySelectorAll(".atem-row .btn")].slice(0, 2).map((b) => b.textContent), ["Edit", "Remove"]);
  });
  button(host, "Add switcher").click();
  await until(() => slot.querySelector('input[aria-label="Address"]'), 3000);
  slot.querySelector('input[aria-label="Address"]').value = "192.0.2.10";
  slot.querySelector('input[aria-label="Name"]').value = "Overflow room";
  button(slot, "Connect").click();
  await until(() => lastArgs(hub, "atem/connect"));
  test("Add switcher connects it under a slug made from its name, following GodwinMix", () =>
    eq(lastArgs(hub, "atem/connect"), { id: "atem-overflow-room", address: "192.0.2.10", name: "Overflow room", mode: "follow" }));
  await until(() => host.querySelectorAll(".atem-row").length === 3, 3000);
  test("and it joins the list", () => eq(host.querySelectorAll(".atem-row").length, 3));
  card.active(false);
  host.remove();

  const { useReady, READY } = await import("../shell/dock-presets.js");
  const { settings, setSetting } = await import("../shell/settings.js");
  const was = settings().producer;
  const layout = localStorage.getItem("gmx.workspace.v1");
  const ws = { state: null, sync() {} };
  useReady(ws, READY[0]);
  if (layout === null) localStorage.removeItem("gmx.workspace.v1");
  else localStorage.setItem("gmx.workspace.v1", layout);
  test("the Volunteer layout is Programme above Sources, in Studio mode, and adds no panel", () => {
    eq([ws.state.tree.a.tabs, ws.state.tree.b.tabs], [["core/program"], ["core/sources"]]);
    eq(settings().producer, true);
  });
  setSetting("producer", was);
}
