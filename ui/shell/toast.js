// Toasts, including the one that offers Undo.
//
// An error toast shows the core's message verbatim, because the message already
// names the current state and the next step. The heading is the caller's, and
// when the core offers a way out (`data.action`) the toast carries it as a
// button. Two codes are reworded, because their messages are written around a
// method name: a missing scope and a missing method.

import { el, clear } from "./dom.js";
import { CODES } from "../client/errors.js";

const DEFAULT_MS = 7000;

let host = null;

function stack() {
  if (!host) {
    host = el("div.toasts", { role: "status", "aria-live": "polite" });
    document.body.appendChild(host);
  }
  return host;
}

/**
 * @param {{text: string, kind?: "info"|"warning"|"error", ms?: number,
 *          action?: {label: string, run: () => void},
 *          actions?: {label: string, run: () => void, after?: number}[]}} opts
 *
 * A button with `after` stays disabled for that many milliseconds, which is
 * how "Try again" waits out the hold the core named.
 */
export function toast(opts) {
  const node = el("div.toast" + (opts.kind ? "." + opts.kind : ""), {}, [
    el("span.grow", { text: opts.text }),
  ]);
  let timer = null;
  const close = () => {
    if (timer) clearTimeout(timer);
    node.remove();
  };
  const buttons = opts.actions || (opts.action ? [opts.action] : []);
  for (const action of buttons) {
    const button = el("button.btn", {
      text: action.label,
      onclick: () => {
        close();
        action.run();
      },
    });
    if (action.after > 0) {
      button.disabled = true;
      setTimeout(() => (button.disabled = false), action.after);
    }
    node.appendChild(button);
  }
  node.appendChild(el("button.btn.icon", { text: "×", title: "Dismiss", onclick: close, "aria-label": "Dismiss" }));
  stack().appendChild(node);
  timer = setTimeout(close, opts.ms === undefined ? DEFAULT_MS : opts.ms);
  return close;
}

/** The action kinds this page can carry out. Anything else is left to the message. */
const KNOWN = new Set(["set-config", "install-plugin", "enable-plugin", "open", "retry", "restart"]);

/**
 * What an error toast says: the heading from the call site, or the code's
 * own when the caller gave none, then the core's sentence. A protocol method
 * name is not for a person, so it goes to the console instead.
 */
export function errorText(err, what) {
  const data = (err && err.data) || {};
  const method = (err && err.method) || data.method;
  if (method) console.debug("refused:", method, err);
  const head = what || (err && err.title) || "That did not work";
  let body = (err && err.message) || "";
  if (err && err.code === CODES.NO_SCOPE) {
    body = `this browser's token is not allowed to do that. It needs the '${data.needed}' permission, which whoever runs the mixer can add to the token.`;
  } else if (err && err.code === CODES.NO_METHOD) {
    body = "this mixer does not have that command. It may be older than this page.";
  }
  return body ? `${head}: ${body}` : head;
}

/**
 * The buttons an error earns: the one the core named in `data.action`, and
 * "Try again" when the core said how long to wait or a plugin died mid call.
 * Only for a failure from `client.call`, which is what knows how to go again.
 */
export function errorButtons(err, what) {
  const out = [];
  const action = err && err.action && KNOWN.has(err.action.kind) ? err.action : null;
  if (action) out.push({ label: action.label, run: () => act(action, err), after: action.after_ms || 0 });
  const canRetry = err && err.again && !(action && action.kind === "retry");
  if (canRetry && (err.retryAfterMs !== null || err.code === CODES.PLUGIN_DIED)) {
    const again = { kind: "retry", label: what || "Try again" };
    out.push({ label: "Try again", run: () => act(again, err), after: err.retryAfterMs || 0 });
  }
  return out;
}

/** The one an RpcError produces. Title, the message the core wrote, and its buttons. */
export function errorToast(err, what) {
  // The person was asked to confirm and said no: nothing went wrong.
  if (err && err.declined) return;
  const actions = errorButtons(err, what);
  const wait = Math.max(0, ...actions.map((a) => a.after || 0));
  toast({ kind: "error", text: errorText(err, what), ms: actions.length ? 20000 + wait : 11000, actions });
}

/**
 * An `alert` event as a toast, with the button the core put on it. Two of
 * them end "restart the mixer", and carry `{kind: "restart"}` to do it.
 */
export function alertToast(client, alert) {
  const actions = alertButtons(client, alert);
  toast({ kind: alert.severity, text: alert.message, ms: alert.severity === "info" ? 6000 : actions.length ? 20000 : 12000, actions });
}

/** The button for an alert's action, or none. Shared with the Alerts panel. */
export function alertButtons(client, alert) {
  const action = alert && alert.action;
  if (!action || !KNOWN.has(action.kind) || typeof action.label !== "string") return [];
  return [{ label: action.label, run: () => act(action, { client }) }];
}

/** The handlers are fetched on the first press, not with the page. */
function act(action, err) {
  import("./error-actions.js")
    .then((m) => m.runAction(action, err))
    .catch((e) => console.error("the action could not run", e));
}

/** For `client.confirm`: the yes or no before a destructive call goes again. */
export function confirmHook(ask) {
  return import("./error-actions.js").then((m) => m.confirmCall(ask));
}

/** "Removed cam2" with an Undo button, which is how Delete stays safe. */
export function undoToast(text, undo) {
  return toast({ text, action: { label: "Undo", run: undo }, ms: 9000 });
}

export function clearToasts() {
  if (host) clear(host);
}
