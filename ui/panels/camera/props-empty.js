// Camera properties for a source no PTZ camera points at yet. With the ptz
// plugin installed it offers to add one for this source; without it, it says
// where to get the plugin and offers simulated cameras to try it with.

import { el } from "../../shell/dom.js";
import { run } from "../../shell/commands.js";
import { errorToast, toast } from "../../shell/toast.js";
import { drawerHead, demoFoot } from "./look.js";

const PROTOCOLS = [
  ["visca-ip", "VISCA over IP"],
  ["visca-tcp", "VISCA over TCP"],
  ["onvif", "ONVIF"],
  ["canon", "Canon"],
];

/** "cam-" and the source's id, as a slug the plugin takes. */
export function slugFor(source) {
  const s = String(source.id).toLowerCase().replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "");
  return /^[a-z]/.test(s) ? (s.startsWith("cam-") ? s : `cam-${s}`) : `cam-${s || "1"}`;
}

/** The video sources the demo can stand in for, this one first. */
function standIns(hub, source) {
  const all = ((hub.real.state && hub.real.state.sources) || []).filter((s) => s.has_video !== false && s.id !== source.id);
  return [source, ...all].slice(0, 3).map((s) => ({ id: s.id, name: s.name || s.id }));
}

export function noCamera(hub, source, reopen) {
  const name = source.name || source.id;
  const head = drawerHead(hub, name, "Camera");
  if (!hub.cameras) {
    return el("div.pad.col.cam-props", {}, [
      head.root,
      el("p.dim", { text: "Steering a PTZ camera needs the ptz plugin. Install it from the plugin list, then point a camera at this source." }),
      el("div.row", {}, [
        el("button.btn.primary", { type: "button", text: "Try with simulated cameras",
          onclick: () => hub.setDemo(true, standIns(hub, source)).then(reopen) }),
        el("button.btn", { type: "button", text: "Plugins", onclick: () => run("view.plugins") }),
      ]),
      el("p.sm.faint", { text: "The simulated cameras stand in for this source and two others, with people walking about, and two simulated ATEM switchers. Nothing real moves." }),
    ]);
  }
  const protocol = el("select", { "aria-label": "How it is spoken to" }, PROTOCOLS.map(([v, t]) => el("option", { value: v, text: t })));
  const address = el("input", { type: "text", placeholder: "192.0.2.20", "aria-label": "Address", spellcheck: "false" });
  const user = el("input", { type: "text", placeholder: "admin", "aria-label": "Login", autocomplete: "off" });
  const password = el("input", { type: "password", "aria-label": "Password", autocomplete: "new-password" });
  const login = el("div.col", { hidden: true }, [el("label.sm.dim", { text: "Login" }, [user]), el("label.sm.dim", { text: "Password" }, [password])]);
  protocol.onchange = () => (login.hidden = !["onvif", "canon"].includes(protocol.value));
  const add = el("button.btn.primary", { type: "button", text: "Add camera" });
  add.onclick = async () => {
    if (!address.value.trim()) return toast({ text: "Type the camera's address first." });
    const set = { id: slugFor(source), protocol: protocol.value, address: address.value.trim(), source: source.id };
    if (!login.hidden && user.value) Object.assign(set, { user: user.value, password: password.value });
    add.disabled = true;
    try {
      await hub.api.add(set);
      toast({ text: `${name} now steers its camera.` });
      reopen();
    } catch (e) {
      errorToast(e, "Add camera");
    } finally {
      add.disabled = false;
    }
  };
  return el("div.pad.col.cam-props", {}, [
    head.root,
    el("p.dim", { text: `No PTZ camera points at ${name} yet. Say how to reach the camera that shows it.` }),
    el("label.sm.dim", { text: "Camera" }, [protocol]),
    el("label.sm.dim", { text: "Address" }, [address]),
    login,
    el("div.row", {}, [add]),
    demoFoot(hub),
  ]);
}
