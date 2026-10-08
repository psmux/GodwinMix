// The params a built in source kind publishes, for its settings form.
//
// A plugin's form comes from `plugin.describe`. A kind built into the core
// publishes its params schema in `core.api`, under `kinds.source`, and the
// drawer used to ignore it, so a clip's settings were its name and nothing
// else. Merged over the page's own form for the kind here: only properties
// the form can draw (a plain `type`), and never one the page already has.
//
// Its own module, with nothing imported, so the tests can reach it.

const asked = new WeakMap();

/** The `params` schema `core.api` publishes for a source `type`, or null. */
export async function coreParams(client, type) {
  if (!type || !client) return null;
  if (!asked.has(client)) asked.set(client, client.call("core.api", {}).catch(() => null));
  const api = await asked.get(client);
  const kinds = (api && api.kinds && api.kinds.source) || [];
  const found = kinds.find((k) => k.id === type);
  return (found && found.params) || null;
}

/** `schema` with the drawable properties of `params` added. */
export function withParams(schema, params) {
  const extra = (params && params.properties) || {};
  const properties = Object.assign({}, (schema && schema.properties) || {});
  for (const [key, value] of Object.entries(extra)) {
    if (key in properties || !value || typeof value.type !== "string") continue;
    properties[key] = value;
  }
  return Object.assign({}, schema, { properties });
}
