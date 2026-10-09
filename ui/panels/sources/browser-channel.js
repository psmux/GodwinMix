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
  const fresh = async () => {
    const added = await client.call("channel.key.add", { id: channel.id, label: "This browser" });
    return { channel, key: added.key.secret };
  };
  if (!first) return fresh();
  // A key the secret store no longer has (a moved or reset mixer home) cannot
  // be shown, and nothing can publish with it, so this browser makes its own.
  const revealed = await client.call("channel.key.reveal", { id: channel.id, key: first.id }).catch((e) => {
    if (e && e.data && e.data.reason === "unsealed") return null;
    throw e;
  });
  return revealed ? { channel, key: revealed.secret } : fresh();
}

// The stream name, one per device, lives with the publisher because /join/
// on a phone needs it too, with no mixer page around it.
export { streamNameFor, deviceName } from "../../join/name.js";

/** The source a live stream becomes: `<app>-<stream>`, as the channel names it. */
export function sourceIdFor(channel, stream) {
  return `${(channel && channel.app) || CHANNEL}-${stream}`;
}

/** The stream as the channel last described it, or null. */
export function streamOf(channel, name) {
  return ((channel && channel.streams) || []).find((s) => s.name === name) || null;
}

/**
 * The camera card's line above the picture: what the mixer has made of the
 * stream `s`. A stream the mixer would not make a source says why, or the
 * card reads live while nothing can reach a scene.
 */
export function dockNote(active, inMixer, s, source) {
  if (!inMixer && s && s.source_error) {
    return `The mixer has the stream but could not make it a source, so it cannot go in a scene. ${s.source_error}`;
  }
  if (active && inMixer) return `In the mixer as ${source}.`;
  if (active && s && s.state === "live") return `The channel has the stream; ${source} is on its way.`;
  return active ? "Sending to the mixer." : "";
}

/**
 * Make sure the ingest plugin, which takes a browser's stream in, is on this
 * mixer and running: install it or switch it on when it is not. `say` is told
 * what is happening, because an install can take a minute.
 */
export async function ensureIngest(client, say = () => {}) {
  const { listPlugins, pluginState } = await import("../../client/kinds.js");
  const plugins = await listPlugins(client);
  const state = pluginState(plugins, "ingest");
  if (state === "ready") return;
  if (state === "problem") {
    const p = plugins.find((x) => x.name === "ingest");
    console.warn("the ingest plugin did not start", p.problem);
    throw new Error("Taking in this browser's camera did not start on the mixer. Restart the mixer, and if it happens again, its log says why.");
  }
  // Installed from the copy the mixer carries, or switched back on.
  const { setUp } = await import("../../client/setup.js");
  await setUp(client, "ingest", say);
}
