// Toasts, including the one that offers Undo.
//
// An error toast shows the core's message verbatim, because the message already
// names the current state and the next step. Nothing here rewords it; the title
// is the only thing this file chooses.

import { el, clear } from "./dom.js";

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
 *          action?: {label: string, run: () => void}}} opts
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
  if (opts.action) {
    node.appendChild(
      el("button.btn", {
        text: opts.action.label,
        onclick: () => {
          close();
          opts.action.run();
        },
      })
    );
  }
  node.appendChild(el("button.btn.icon", { text: "×", title: "Dismiss", onclick: close, "aria-label": "Dismiss" }));
  stack().appendChild(node);
  timer = setTimeout(close, opts.ms === undefined ? DEFAULT_MS : opts.ms);
  return close;
}

/** The one an RpcError produces. Title, then the message the core wrote. */
export function errorToast(err, what) {
  const lead = what ? `${what}: ` : "";
  toast({ kind: "error", text: lead + (err.message || err.title), ms: 11000 });
}

/** "Removed cam2" with an Undo button, which is how Delete stays safe. */
export function undoToast(text, undo) {
  return toast({ text, action: { label: "Undo", run: undo }, ms: 9000 });
}

export function clearToasts() {
  if (host) clear(host);
}
