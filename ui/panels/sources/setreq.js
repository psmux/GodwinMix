// What the source drawer is allowed to send, and where each field goes.
//
// `source.set` takes a fixed set of fields and merges anything else into
// `params`. A kind's schema is written for `source.add`, so it carries `uri`
// as well, and `uri` is not settable on a source that already exists.
//
// Both halves of this matter. Showing a field the core will not act on is the
// bug this exists to stop: the call answers 200 either way, so an address
// typed into a form that drops it looks exactly like one that was saved.
//
// Its own module, with nothing imported, so the tests can reach it without
// pulling the shell in behind it.

const SET_SOURCE_FIELDS = ["name", "color", "place", "transport", "latency_ms"];

/** The kind's schema with the fields `source.set` cannot act on taken out. */
export function settableOnly(schema) {
  const props = (schema && schema.properties) || {};
  if (!("uri" in props)) return schema;
  const properties = Object.assign({}, props);
  delete properties.uri;
  return Object.assign({}, schema, {
    properties,
    required: (schema.required || []).filter((r) => r !== "uri"),
  });
}

/** A `source.set` request: the known fields at the top, the rest under `params`. */
export function setRequest(id, values) {
  const req = { id };
  const params = {};
  for (const [key, value] of Object.entries(values)) {
    if (value === undefined) continue;
    if (SET_SOURCE_FIELDS.includes(key)) req[key] = value;
    else params[key] = value;
  }
  if (Object.keys(params).length) req.params = params;
  return req;
}
