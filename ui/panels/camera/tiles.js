// The Sources panel's side of the cameras: a source a PTZ camera points at
// gets a camera button beside the gear, as FX sits on an audio row, and a
// quiet badge on its picture while the camera steers by itself. The panel
// fetches this after it first draws and calls paint() after each render;
// with no ptz plugin and no demo it adds nothing to any tile and costs a
// plugin.list now and then.

import { el } from "../../shell/dom.js";
import { hubFor } from "./hub.js";
import { styled, cameraIcon } from "./look.js";
import { badgeWords, trackWords } from "./words.js";

/** The camera button and the badge, put on a tile the first time it needs them. */
function dress(panel, id, tile) {
  const button = el("button.gear.camera", { type: "button", title: "Camera", "aria-label": "Camera", "data-nodrag": "" }, [cameraIcon(14)]);
  button.onclick = (e) => {
    e.stopPropagation();
    panel.openCamera(id);
  };
  const gear = tile.node.querySelector(".bar > .gear:not(.interact)");
  gear.before(button);
  const badge = el("span.tile-follow", { hidden: true }, [el("i"), el("span")]);
  tile.node.append(badge);
  return (tile.camera = { button, badge });
}

export function cameraTiles(panel) {
  styled();
  const hub = hubFor(panel.client);
  let off = null;
  const paint = () => {
    for (const [id, tile] of panel.tiles || []) {
      const cam = hub.cameraFor(id);
      if (!cam && !tile.camera) continue;
      const { button, badge } = tile.camera || dress(panel, id, tile);
      button.hidden = !cam;
      const t = cam && hub.track.get(cam.id);
      const word = badgeWords(t);
      badge.hidden = !word;
      if (badge.lastChild.textContent !== word) badge.lastChild.textContent = word;
      badge.className = "tile-follow " + ((t && t.state) || "");
      badge.title = cam ? `${cam.name}: ${trackWords(t)}` : "";
      button.title = cam ? `Camera: ${cam.name}` : "Camera";
    }
  };
  hub.on(paint);
  const parts = {
    paint,
    /** Read while the tray is on screen, and not while it is not. */
    active(on) {
      if (on && !off) off = hub.want("cams");
      if (!on && off) (off(), (off = null));
    },
  };
  parts.active(panel.isConnected && panel.workspaceActive !== false);
  return parts;
}
