// The channel this browser's camera publishes to, through the same channel.*
// methods any client has. Made on first use, put right when somebody has
// changed it, and its key read back with channel.key.reveal.

export const CHANNEL = "browser";

/**
 * The `channel.set` that makes an existing channel take this browser, or
 * null when it already does: switched on, WHIP on, each stream a source, and
 * the key given as the bearer token rather than as the stream name.
 */
export function repairFor(channel) {
  const fix = {};
  if (!channel.enabled) fix.enabled = true;
  if (!channel.auto_source) fix.auto_source = true;
  if (channel.key_mode && channel.key_mode !== "query") fix.key_mode = "query";
  const protocols = channel.protocols || ["rtmp"];
  if (!protocols.includes("whip")) fix.protocols = [...protocols, "whip"];
  return Object.keys(fix).length ? { id: channel.id, ...fix } : null;
}

/** `{channel, key}`: the browser channel and a key that lets this page publish. */
export async function ensureBrowserChannel(client) {
  let channel = await client.call("channel.get", { id: CHANNEL }).catch((e) => {
    if (e && e.code === -32004) return null;
    throw e;
  });
  if (!channel) {
    const made = await client.call("channel.add", { name: "Browser", app: CHANNEL, auto_source: true, protocols: ["whip"] });
    return { channel: made.channel, key: made.key.secret };
  }
  const fix = repairFor(channel);
  if (fix) channel = await client.call("channel.set", fix);
  const first = (channel.keys || [])[0];
  if (!first) {
    const added = await client.call("channel.key.add", { id: channel.id, label: "This browser" });
    return { channel, key: added.key.secret };
  }
  const revealed = await client.call("channel.key.reveal", { id: channel.id, key: first.id });
  return { channel, key: revealed.secret };
}

/**
 * The stream name for this browser: what it is and where it runs, as a
 * slug. "chrome-macos", "firefox-windows", "safari-ios".
 */
export function streamNameFor(ua = navigator.userAgent, platform = navigator.userAgentData?.platform || navigator.platform || "") {
  const u = String(ua);
  const browser = /Edg\//.test(u) ? "edge" : /OPR\//.test(u) ? "opera" : /Firefox\//.test(u) ? "firefox"
    : /Chrome\//.test(u) ? "chrome" : /Safari\//.test(u) ? "safari" : "browser";
  const p = `${platform} ${u}`.toLowerCase();
  const os = /iphone|ipad|ios/.test(p) ? "ios" : /android/.test(p) ? "android" : /mac/.test(p) ? "macos"
    : /win/.test(p) ? "windows" : /cros/.test(p) ? "chromeos" : /linux/.test(p) ? "linux" : "";
  return [browser, os].filter(Boolean).join("-");
}

/** The source a live stream becomes: `<app>-<stream>`, as the channel names it. */
export function sourceIdFor(channel, stream) {
  return `${(channel && channel.app) || CHANNEL}-${stream}`;
}

/** The stream as the channel last described it, or null. */
export function streamOf(channel, name) {
  return ((channel && channel.streams) || []).find((s) => s.name === name) || null;
}

/**
 * Make sure the ingest plugin, which takes a browser's stream in, is on this
 * mixer and running: install it or switch it on when it is not. `say` is told
 * what is happening, because an install can take a minute.
 */
export async function ensureIngest(client, say = () => {}) {
  const { listPlugins, pluginState, pluginSourceFor } = await import("../../client/kinds.js");
  const plugins = await listPlugins(client);
  const state = pluginState(plugins, "ingest");
  if (state === "ready") return;
  if (state === "problem") {
    const p = plugins.find((x) => x.name === "ingest");
    throw new Error(`The ingest plugin, which takes this browser's stream in, is installed but did not start: ${p.problem}. Open Plugins to see why.`);
  }
  if (state === "disabled") {
    say("Switching on the ingest plugin, which takes this browser's stream in.");
    await client.call("plugin.enable", { name: "ingest" });
    return;
  }
  say("Installing the ingest plugin, which takes this browser's stream in. This can take a minute.");
  await client.call("plugin.add", { source: await pluginSourceFor(client, "ingest") });
}
