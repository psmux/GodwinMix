// What the button under an error toast does.
//
// A refusal that knows its way out carries it as `data.action` (see
// docs/reference/errors.md): change one setting, install or turn on a plugin,
// open a part of the page, try again, or restart the mixer. The toast draws
// the button from the label alone; this file is fetched the first time one is
// pressed, so a page where nothing ever goes wrong never pays for it.

import { toast, errorToast } from "./toast.js";
import { listPlugins, pluginSourceFor, pluginState } from "../client/kinds.js";

/** Run one action for the error it came with. Never throws. */
export async function runAction(action, err) {
  const client = err && err.client;
  if (!client) return;
  try {
    switch (action.kind) {
      case "set-config":
        return await setConfig(client, action, err);
      case "install-plugin":
        return await installPlugin(client, action.name, err);
      case "enable-plugin":
        return await enablePlugin(client, action.name, err);
      case "open":
        return await openPart(client, action);
      case "retry":
        return await again(err, action.label);
      case "restart":
        return await restart(client);
      default:
        console.debug("an action this page does not know", action);
    }
  } catch (e) {
    errorToast(e, action.label);
  }
}

/**
 * `config.set` with the one value. A live key has done its work by the time
 * the call answers, so the call that was refused goes again; a restart key
 * says so and offers the restart where the mixer can do one.
 */
async function setConfig(client, action, err) {
  const answer = await client.call("config.set", { values: { [action.key]: action.value } });
  const waiting = (answer && answer.needs_restart) || [];
  if (waiting.includes(action.key)) return offerRestart(client, "Saved. It takes effect when the mixer restarts.");
  if (err.again) return again(err, action.label);
  toast({ text: "Done." });
}

async function installPlugin(client, name, err) {
  const note = toast({ text: `Installing ${name}. This can take a minute.`, ms: 60000 });
  const source = await pluginSourceFor(client, name);
  try {
    await client.call("plugin.add", { source });
  } finally {
    note();
  }
  const state = pluginState(await listPlugins(client), name);
  if (state === "ready" && err.again) return again(err, `Installed ${name}`);
  toast({ text: state === "ready" ? `${name} is installed.` : `${name} was installed, but the mixer has not picked it up yet.` });
}

async function enablePlugin(client, name, err) {
  await client.call("plugin.enable", { name });
  if (err.again) return again(err, `Turned ${name} on`);
  toast({ text: `${name} is on.` });
}

/** Settings is the one dialog a core names today; a panel is focused by id. */
async function openPart(client, action) {
  if (action.dialog === "settings") {
    const { openSettings } = await import("./settings.js");
    return openSettings(client, { key: action.key });
  }
  const panel = action.panel && document.querySelector(`[data-panel="${CSS.escape(action.panel)}"]`);
  if (panel) panel.scrollIntoView({ block: "nearest" });
}

/** The refused call, sent again. Success says so briefly; a failure is a new toast. */
async function again(err, label) {
  try {
    await err.again();
    toast({ text: `${label}: done.` });
  } catch (e) {
    errorToast(e, label);
  }
}

/**
 * A restart where the mixer can come back by itself, and a plain sentence
 * where it cannot. `core.info` is asked each time, because a mixer started
 * by hand today may run under the service tomorrow.
 */
export async function offerRestart(client, lead) {
  const info = await client.call("core.info", {}).catch(() => null);
  if (info && info.restart && info.restart.possible) {
    return toast({ text: lead, ms: 20000, action: { label: "Restart now", run: () => restart(client) } });
  }
  toast({ text: `${lead} Stop the mixer and start it again when the show allows.`, ms: 20000 });
}

async function restart(client) {
  const answer = await client.call("core.restart", {});
  toast({ text: (answer && answer.message) || "The mixer is restarting. This page reconnects by itself.", ms: 12000 });
}

/**
 * The yes or no before a destructive call on a token that asks for one.
 * The core's own sentence names the method and the token, which is for a
 * log; the person is asked in words about what they pressed.
 */
export async function confirmCall({ method, params }) {
  console.debug("confirm required for", method);
  const { confirmModal } = await import("./modal.js");
  const doing = DOING[method] ? DOING[method](params || {}) : "That change";
  return confirmModal(`${doing} needs a second yes on this mixer. Go ahead?`, "Go ahead");
}

/** The destructive methods, in words, with the id where there is one. */
const named = (lead) => (p) => (p.id || p.name ? `${lead} ${p.id || p.name}` : lead);
const DOING = {
  "config.reset": () => "Putting a mixer setting back",
  "config.set": () => "Changing a mixer setting",
  "core.restart": () => "Restarting the mixer",
  "core.shutdown": () => "Stopping the mixer",
  "filter.remove": named("Removing the filter"),
  "media.remove": named("Deleting"),
  "node.remove": named("Removing the node"),
  "output.remove": named("Removing the output"),
  "plugin.add": (p) => `Installing ${p.source || "a plugin"}`,
  "plugin.remove": named("Removing the plugin"),
  "plugin.update": named("Updating the plugin"),
  "preset.apply": named("Applying the preset"),
  "scene.item.filter.remove": () => "Removing a filter from the scene",
  "scene.item.remove": () => "Removing an item from the scene",
  "scene.remove": named("Removing the scene"),
  "source.remove": named("Removing the source"),
};
