// The Sources panel's side of the cameras: a source a PTZ camera points at
// gets its camera button beside the gear, and a quiet badge on its picture
// while the camera is steering by itself. The panel fetches this after it
// first draws; with no ptz plugin and no demo it shows nothing and costs a
// plugin.list now and then.

import { hubFor } from "./hub.js";
import { styled } from "./look.js";
import { badgeWords, trackWords } from "./words.js";

export function cameraTiles(panel) {
  styled();
  const hub = hubFor(panel.client);
  let off = null;
  const paint = () => {
    for (const [id, tile] of panel.tiles || []) {
      const cam = hub.cameraFor(id);
      if (!tile.camera) continue;
      tile.camera.hidden = !cam;
      tile.node.classList.toggle("ptz", !!cam);
      const t = cam && hub.track.get(cam.id);
      const word = badgeWords(t);
      tile.follow.hidden = !word;
      if (tile.follow.lastChild.textContent !== word) tile.follow.lastChild.textContent = word;
      tile.follow.className = "tile-follow " + ((t && t.state) || "");
      tile.follow.title = cam ? `${cam.name}: ${trackWords(t)}` : "";
    }
  };
  const offHub = hub.on(paint);
  return {
    paint,
    /** Read while the tray is on screen, and not while it is not. */
    active(on) {
      if (on && !off) off = hub.want("cams");
      if (!on && off) (off(), (off = null));
    },
    destroy() {
      offHub();
      off?.();
      off = null;
    },
  };
}
