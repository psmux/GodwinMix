// Camera properties, in the side drawer, for the source a PTZ camera points
// at: its live picture with the people the tracker sees (tap one to follow
// them), the joystick and zoom, the framing chips and the Track switch, the
// saved shots, and exposure and focus folded away at the foot. Opened from
// the camera button on a source tile or Camera on its menu, the way the gear
// opens a source's settings. Fetched on that first click and not before.
//
// Nothing here takes anything to air: the tile puts the camera in Preview,
// and Take and Cut send it, as for any other source.

import { el } from "../../shell/dom.js";
import { shell } from "../../shell/shell.js";
import { run } from "../../shell/commands.js";
import { hubFor } from "./hub.js";
import { Drive } from "./drive.js";
import { controls } from "./controls.js";
import { livePicture } from "./live.js";
import { presetGrid } from "./presets.js";
import { exposureFocus } from "./exposure.js";
import { gamepads } from "./gamepad.js";
import { trackWords } from "./words.js";
import { styled, actor, drawerHead, demoFoot } from "./look.js";

/** What each camera last said about its exposure and focus, on this page. */
const said = new Map();

export async function openCamera(client, source) {
  styled();
  const hub = hubFor(client);
  await hub.ready;
  await hub.plugins(0);
  const off = hub.want("cams");
  await hub.refresh();
  const cam = hub.cameraFor(source.id);
  if (cam) return shell.drawer(cameraProps(hub, source, cam, off));
  off();
  const { noCamera } = await import("./props-empty.js");
  shell.drawer(noCamera(hub, source, () => openCamera(client, source)));
}

function cameraProps(hub, source, cam, offCams) {
  const id = cam.id;
  const act = actor(hub);
  const drive = new Drive({ move: (c, p, t, z) => hub.api.move(c, p, t, z), still: (c) => hub.api.still(c) });
  drive.onError = act.fail;
  drive.use(id);
  const hand = controls(hub, () => id, drive, act);
  const words = el("span.cam-words");
  const dot = el("span.cam-trackdot");
  const remember = (part) => said.set(id, { ...(said.get(id) || {}), ...part });
  const presets = presetGrid({
    recall: (p) => act(() => hub.api.presetRecall(id, p.slot)),
    save: (slot) => act(async () => {
      await hub.api.presetSave(id, slot, `Shot ${slot}`);
      await hub.loadPresets(id);
    }),
  });
  const more = exposureFocus({
    exposure: (e) => act(async () => {
      const r = (await hub.api.exposure(id, e)) || {};
      const known = Object.fromEntries(["mode", "iris", "shutter", "gain"].filter((k) => r[k] !== undefined && r[k] !== null).map((k) => [k, r[k]]));
      remember({ exposure: { ...((said.get(id) || {}).exposure || {}), ...e, ...known } });
    }),
    focus: (mode, speed) => act(async () => {
      await hub.api.focus(id, mode, speed);
      if (!["near", "far", "stop"].includes(mode)) remember({ focus: { mode: mode === "auto" ? "auto" : "manual" } });
    }),
  });
  const pad = gamepads({
    move: (p, t) => drive.move(p, t),
    zoom: (v) => drive.zoom(v),
    stop: () => drive.stopAll(),
    take: () => run("program.take-armed"),
    cut: () => run("program.cut-armed"),
  });
  const offs = [offCams, hub.want("people:" + id)];
  const pic = livePicture(hub, () => hub.cam(id) || cam, act, { onGone: () => gone() });
  const head = drawerHead(hub, source.name || source.id, "Camera");

  const root = el("div.pad.col.cam-props", { "data-camera": id }, [
    head.root,
    el("div.cam-pic", {}, [pic.canvas]),
    el("div.row.cam-state", {}, [dot, words, el("span.grow"), el("span.sm.faint", { text: "Tap a person to follow them" })]),
    el("div.cam-hand", {}, [hand.stick.root, el("div.col.cam-auto", {}, [hand.track.root, hand.chips])]),
    hand.zoom.root,
    presets.root,
    more.root,
    demoFoot(hub),
  ]);

  function sync() {
    const t = hub.track.get(id);
    hand.sync(t);
    const text = trackWords(t);
    if (words.textContent !== text) words.textContent = text;
    dot.className = "cam-trackdot " + ((t && t.on && t.state) || "off");
    head.tally(hub.real.store ? hub.real.store.tallyOf(source.id) : "off");
    presets.show(hub.cam(id) || cam, hub.presets.get(id));
    more.show(said.get(id));
  }

  function gone() {
    hand.release();
    drive.destroy();
    pad.destroy();
    offModel();
    offRender?.();
    for (const off of offs) off();
  }

  const offModel = hub.on(sync);
  const offRender = hub.real.onRender ? hub.real.onRender(() => sync()) : null;
  pic.start();
  pad.start();
  hub.loadPresets(id);
  sync();
  return root;
}
