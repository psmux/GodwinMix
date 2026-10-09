// What the menu's own commands do. Everything else a menu item runs is a
// command some panel or the shell already had.

import { el } from "./dom.js";
import { run, get } from "./commands.js";
import { settings, setSetting } from "./settings.js";
import { applyTheme, current as currentTheme } from "./theme.js";
import { leaves } from "./dock-model.js";
import { modal } from "./modal.js";
import { openPicker } from "./picker-loader.js";
import { toast } from "./toast.js";

const DOCS = "https://github.com/psmux/GodwinMix/tree/main/docs";

const workspace = () => document.querySelector("gmx-shell")?.workspace || null;

/** Whether a panel is on screen: in the layout and not closed. */
function showing(id) {
  const w = workspace();
  return !!w && leaves(w.state.tree).some((g) => g.tabs.includes(id));
}

/** The tick beside a View item. */
export function checked(item) {
  if (item.check === "panel") return showing(item.arg);
  if (item.check === "studio") return !!settings().producer;
  if (item.check === "theme") return currentTheme() === item.arg;
  if (item.check === "routing") return !!document.querySelector(".routing");
  if (item.check === "wall") return !!document.querySelector(".wall");
  return null;
}

/** Show a panel and wait for it to be drawn, so its commands exist. */
async function show(id) {
  const w = workspace();
  if (!w) return;
  w.show(id);
  await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
}

const project = () => import("./project.js");
const shows = () => import("./show-file.js");

const ACTIONS = {
  "project.new": (client) => project().then((m) => m.newProject(client)),
  "project.open": (client) => project().then((m) => m.openProject(client)),
  "project.save": (client) => project().then((m) => m.saveProject(client)),
  "show.new": (client) => shows().then((m) => m.newShow(client)),
  "show.switch": (client, id) => shows().then((m) => m.switchShow(client, id)),
  "view.routing": (client) => import("../panels/routing/view.js").then((m) => m.toggleRouting(client)),
  "view.wall": (client) => import("../panels/wall/view.js").then((m) => m.toggleWall(client)),
  "show.add-many": (client) => import("../panels/wall/bulk.js").then((m) => m.bulkAdd(client)),
  "edit.delete": () => {
    const which = ["tray.delete", "scenes.remove"].find((id) => {
      const cmd = get(id);
      return cmd && (!cmd.enabled || cmd.enabled());
    });
    if (which) return run(which);
    toast({ text: "Nothing is selected. Select a source or a scene first." });
  },
  "view.show": (client, id) => show(id),
  "view.panel": (client, id) => {
    const w = workspace();
    if (!w) return;
    if (showing(id)) w.hide(id);
    else w.show(id);
  },
  "view.studio": () => setSetting("producer", !settings().producer),
  "view.reset-layout": () => workspace()?.reset(),
  "view.theme": (client, id) => applyTheme(id),
  "source.add-kind": (client, category) => openPicker(client, "source", { category }),
  "data.open": (client) => import("../panels/data/dialog.js").then((m) => m.openLiveData(client)),
  "output.record": (client) => import("../panels/outputs/recording.js").then((m) => m.startRecording(client)),
  "output.stop-recording": (client) => import("../panels/outputs/record-stop.js").then((m) => m.stopRecording(client)),
  "output.running": (client) => import("../panels/header/running.js").then((m) => m.openRunning(client)),
  "output.resources": async () => {
    await show("core/outputs");
    document.querySelector("gmx-outputs")?.show("resources");
  },
  "help.devices": (client) => import("./devices.js").then((m) => m.openDevices(client)),
  "help.agents": (client) => import("./agents.js").then((m) => m.openAgents(client)),
  "help.docs": () => window.open(DOCS, "_blank", "noopener"),
  "help.about": (client) => about(client),
};

export function act(client, id, arg) {
  const fn = ACTIONS[id];
  return fn ? fn(client, arg) : undefined;
}

async function about(client) {
  const info = await client.call("core.info", {}).catch(() => ({}));
  const canvas = info.canvas ? `${info.canvas.width}×${info.canvas.height} at ${info.canvas.fps} fps` : "";
  modal({
    title: "About GodwinMix",
    body: el("div.col", {}, [
      el("p", { text: `GodwinMix ${info.version || ""}, protocol level ${info.api_level ?? "?"}.` }),
      canvas ? el("p.dim", { text: `The programme is ${canvas}.` }) : null,
      el("p.dim", { text: "A live video mixer. This page talks to the mixer over the same protocol every other client uses." }),
      el("p.sm.dim", {}, ["NDI® is a registered trademark of Vizrt NDI AB. ", el("a", { href: "https://ndi.video/", target: "_blank", rel: "noopener", text: "ndi.video" })]),
      el("a", { href: DOCS, target: "_blank", rel: "noopener", text: "Documentation" }),
    ]),
  });
}
