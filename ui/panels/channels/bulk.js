// The small menu on a channel's "Send on to" header: start every push
// destination, stop every one, or paste several addresses at once. Start and
// Stop are one `channel.destination.set` per destination, the same call the
// tile's switch makes, so there is nothing here a third party could not do.

import { el } from "../../shell/dom.js";
import { contextMenu } from "../../shell/menu.js";
import { confirmModal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { pasteAddresses } from "./paste.js";
import { isLocal } from "./local.js";

// Push destinations only, as Livebox's Turn ON all and Turn OFF all were: a
// recording or a watch link is not stopped by a Stop all meant for platforms.
const pushes = (channel) => (channel.destinations || []).filter((d) => !isLocal(d.platform));

export function bulkButton(view, getChannel) {
  const button = el("button.btn.sm.chn-bulk", { type: "button", text: "Bulk actions", title: "Start all, stop all, or paste several addresses", "aria-haspopup": "menu" });
  button.onclick = () => {
    const r = button.getBoundingClientRect();
    contextMenu(r.left, r.bottom + 4, bulkItems(view, getChannel()));
  };
  return button;
}

/** The menu's entries for one channel, with the ones that would do nothing greyed. */
export function bulkItems(view, channel) {
  const list = pushes(channel);
  return [
    { label: "Start all", disabled: !list.some((d) => !d.enabled), run: () => setAll(view, channel, true) },
    { label: "Stop all", disabled: !list.some((d) => d.enabled), run: () => stopAll(view, channel) },
    { kind: "separator" },
    { label: "Paste several addresses", run: () => pasteAddresses(view, channel) },
  ];
}

async function stopAll(view, channel) {
  const live = pushes(channel).filter((d) => d.enabled && d.state === "live").length;
  if (live) {
    const name = channel.name || channel.id;
    const words = live === 1 ? "1 push destination is live" : `${live} push destinations are live`;
    if (!(await confirmModal(`${words} on ${name}. Stop sending to every push destination of this channel? Recording and the watch link carry on.`, "Stop all"))) return;
  }
  return setAll(view, channel, false);
}

/**
 * Switch every destination of a channel on or off, one call each. The ones
 * already that way are left alone. Failures are counted and named in one toast.
 */
export async function setAll(view, channel, enabled) {
  const todo = pushes(channel).filter((d) => !!d.enabled !== enabled);
  const failed = [];
  for (const d of todo) {
    try {
      view.accept(await view.client.call("channel.destination.set", { id: channel.id, destination: d.id, enabled }));
    } catch (e) {
      failed.push(`${d.label || d.id}: ${(e && e.message) || e}`);
    }
  }
  const verb = enabled ? "start" : "stop";
  if (failed.length) toast({ kind: "warning", text: `Could not ${verb} ${failed.length} of ${todo.length}. ${failed.join(" ")}` });
  return { done: todo.length - failed.length, failed };
}
