// Adding and editing a destination, for somebody who has never seen an RTMP
// URL and is not going to start now.
//
// The rule this exists for: a GUI user never edits a TOML file. Before it, a
// preset wrote `rtmp://a.rtmp.youtube.com/live2/YOUR-STREAM-KEY` into the
// config and the only way to put a real key in was a text editor on the box.
//
// So: pick the platform, paste the key. The server address comes from the
// table in kinds.js, the id defaults to the platform's slug, and the key is a
// `format: "secret"` field, which the schema form renders as a password input
// and leaves out of its answer unless somebody retyped it.
//
// That last part is what makes the edit form possible at all. The core never
// hands an address back, deliberately, so an edit that does not touch the key
// has to send no address, and `output.set` reads an absent `uri` as "keep the
// one in force". Nothing here ever has to hold a key it did not just take off
// the clipboard.

import { el, on } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast, errorToast } from "../../shell/toast.js";
import { PLATFORMS, platform, platformOfHost, joinKey } from "../../client/kinds.js";

/**
 * The platforms this build can actually reach. The core says which output
 * kinds exist, so a build with no `srtsink` does not get an SRT tile it would
 * only fail on.
 */
export async function reachable(client) {
  try {
    const api = await client.call("core.api", {});
    const have = new Set(((api.kinds && api.kinds.output) || []).map((k) => k.id));
    if (have.size) return PLATFORMS.filter((p) => have.has(p.provides));
  } catch {
    /* an older core, or none: offer the table and let the add say why */
  }
  return PLATFORMS;
}

/**
 * "Add destination": one tile per platform, then that platform's form.
 *
 * @param {object} client
 * @param {{onDone?: () => any}} opts
 */
export async function addDestination(client, opts = {}) {
  const kinds = await reachable(client);
  const grid = el("div.picker-grid.list.destinations");
  const m = modal({
    title: "Add destination",
    body: el("div", {}, [
      el("p.dim.sm", {
        text: "Pick where the service is going. For the big platforms all you need is the stream key.",
        style: { marginTop: "0" },
      }),
      grid,
    ]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() })],
    wide: true,
  });
  for (const p of kinds) {
    grid.appendChild(
      el("button.kindtile", {
        onclick: () => {
          m.close();
          openForm(client, p, null, opts.onDone);
        },
      }, [el("span.grow", {}, [el("div", { text: p.title }), el("div.dim.sm", { text: p.where })])])
    );
  }
  return m;
}

/**
 * Change a destination that already exists, with the platform recognised from
 * the masked host, which is all a client is ever given.
 */
export function editDestination(client, output, opts = {}) {
  return openForm(client, platformOfHost(output.uri_host), output, opts.onDone);
}

/**
 * The form's schema. Built per platform and per add or edit rather than kept
 * as a constant, because what is asked for genuinely differs: an add needs a
 * name and a whole address, an edit needs neither unless the key is changing.
 *
 * @param {object} p       a row from PLATFORMS
 * @param {object|null} output  the OutputStatus being edited, or null
 */
export function schemaFor(p, output) {
  const editing = !!output;
  const props = {};
  if (!editing) {
    props.id = {
      type: "string",
      title: "Name",
      default: p.id,
      description: "A short id. It appears in alerts and in the outputs list.",
    };
  }
  props.server = {
    type: "string",
    title: p.key ? "Server" : "Address",
    // A published ingest is known whether this is an add or an edit, so the
    // usual edit, replacing a key on YouTube, needs nothing typed but the key.
    // A server nobody published is not knowable from `uri_host`: the part that
    // is masked is exactly the part that would be needed.
    default: p.fixed ? p.server : editing ? undefined : p.server || undefined,
    examples: [editing && !p.fixed ? output.uri_host : p.server || "rtmp://your.server/live"],
    description: editing
      ? "Kept as it is unless you change the key, which rebuilds the whole address."
      : "Where the platform takes the stream. Paste a different one over it if you were given one.",
  };
  if (p.key) {
    props.key = {
      type: "string",
      format: "secret",
      title: "Stream key",
      examples: [editing ? "kept" : "paste it here"],
      description: "It is never shown again once it is saved.",
    };
  }
  props.policy = {
    type: "string",
    title: "When it drops",
    // "keep" only exists on an edit, and it is the default there. A select
    // always answers with something, and the record does not carry the policy
    // in force, so without it every edit would quietly reset a cdn output to
    // own on its way to changing the buffer.
    enum: editing ? ["keep", "own", "cdn"] : ["own", "cdn"],
    default: editing ? "keep" : p.policy,
    description:
      "own: reconnect on our schedule, for a server you run. cdn: back off the way the " +
      "big platforms want." + (editing ? " keep: leave it as it is." : ""),
    "x-gmx-group": "Advanced",
  };
  props.queue_secs = {
    type: "number",
    title: "Outage buffer",
    // No default on an edit. `queue_secs` on the record is how full the buffer
    // is right now, not how deep it was configured, so filling the box with it
    // would quietly resize the buffer to whatever a glance happened to catch.
    default: editing ? undefined : 4,
    minimum: 0,
    maximum: 60,
    "x-gmx-unit": "s",
    "x-gmx-group": "Advanced",
    description: "How much encoded video to hold, so a short drop is invisible to the viewer.",
  };
  const required = editing ? [] : p.key ? ["id", "server", "key"] : ["id", "server"];
  return { type: "object", required, properties: props };
}

/**
 * The form's answers as the params of the call that will be made, or an error
 * to put in front of the operator.
 *
 * The address is only ever sent when there is a whole one to send. An edit
 * that changed neither half sends no `uri` at all, which is what keeps the key
 * where it is while the buffer or the policy moves.
 */
export function paramsFor(p, output, values) {
  const editing = !!output;
  const id = String(values.id !== undefined ? values.id : output ? output.id : "").trim();
  if (!id) return { error: "Give it a name first." };

  const server = String(values.server || "").trim();
  // The key arrives trimmed here too. One pasted out of a platform's dashboard
  // picks up a newline often enough that trusting the paste is a support call.
  const key = values.key === undefined ? undefined : String(values.key).trim();
  const params = { id };

  if (!editing) {
    if (!server) return { error: p.key ? "Fill in the server address." : "Fill in the address." };
    if (p.key && !key) return { error: "Paste the stream key." };
    params.uri = p.key ? joinKey(server, key) : server;
  } else if (p.key && key !== undefined) {
    if (!server) {
      return { error: "Fill in the server address as well, so the whole address can be rebuilt." };
    }
    params.uri = joinKey(server, key);
  } else if (p.key && server !== prefilledServer(p)) {
    // A server changed on its own, a regional Twitch ingest for instance. The
    // key is half of the address and this form cannot read it back, so say
    // what is missing rather than sending half an address or dropping the
    // change without a word.
    return {
      error: "Press Replace key and paste the key as well, so the whole address can be rebuilt.",
    };
  } else if (!p.key && server) {
    params.uri = server;
  }

  if (values.policy !== undefined && values.policy !== "keep") params.policy = values.policy;
  if (values.queue_secs !== undefined) params.queue_secs = values.queue_secs;
  return { params };
}

/** What `schemaFor` puts in the server box on an edit, so a change shows. */
function prefilledServer(p) {
  return p.fixed ? p.server : "";
}

async function openForm(client, p, output, onDone) {
  const { SchemaForm } = await import("../../client/schema-form.js");
  const editing = !!output;
  const form = new SchemaForm(schemaFor(p, output), {});
  const save = el("button.btn.primary", { text: editing ? "Save" : "Start sending" });
  const m = modal({
    title: editing ? `Edit ${output.id}` : p.title,
    body: el("div", {}, [el("p.dim.sm", { text: p.where, style: { marginTop: "0" } }), form.el]),
    footer: [el("button.btn", { text: "Cancel", onclick: () => m.close() }), save],
  });

  // The key on an edit is locked until somebody says so. The field is already
  // safe, because an untouched secret is left out of the answer, but a live
  // destination's key is worth one deliberate press before it can be typed
  // over by a stray keystroke on the way to the buffer.
  const keyField = p.key && editing ? form.fields.find((f) => f.name === "key") : null;
  if (keyField) {
    keyField.input.disabled = true;
    const replace = el("button.btn.icon.sm.replace-key", {
      text: "Replace key",
      onclick: () => {
        keyField.input.disabled = false;
        replace.remove();
        keyField.input.focus();
      },
    });
    keyField.wrap.appendChild(replace);
  }

  on(form.el, "keydown", (e) => {
    if (e.key === "Enter" && e.target.tagName === "INPUT") {
      e.preventDefault();
      save.click();
    }
  });

  save.onclick = async () => {
    if (!form.validate()) {
      toast({ kind: "warning", text: "Fill in the fields outlined in red." });
      return;
    }
    const asked = paramsFor(p, output, form.read());
    if (asked.error) {
      toast({ kind: "warning", text: asked.error });
      return;
    }
    save.disabled = true;
    try {
      await client.call(editing ? "output.set" : "output.add", asked.params);
    } catch (e) {
      errorToast(e, editing ? "Save" : p.title);
      save.disabled = false;
      return;
    }
    m.close();
    toast({ text: editing ? `${asked.params.id} saved. It reconnects now.` : "Sending started." });
    if (onDone) await onDone();
  };

  form.focusFirst();
  return m;
}
