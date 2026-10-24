// A show's name changed in place on its tab: Enter keeps it, Escape puts it
// back, leaving the box keeps it. Fetched with the rest of show-actions.js.

import { el } from "./dom.js";
import { errorToast } from "./toast.js";

/** The name, in place, as an input. */
export function rename(view, show) {
  const tab = view.list.querySelector(`[data-show="${CSS.escape(show.id)}"]`);
  const name = tab && tab.querySelector(".showtab-name");
  if (!name) return;
  const input = el("input.showtab-input", { type: "text", value: show.name, "aria-label": `New name for ${show.name}`, size: Math.max(6, show.name.length) });
  name.replaceWith(input);
  input.select();
  let done = false;
  const finish = async (keep) => {
    if (done) return;
    done = true;
    const next = input.value.trim();
    if (keep && next && next !== show.name) {
      try {
        await view.client.call("show.rename", { id: show.id, name: next });
      } catch (e) {
        errorToast(e, "Rename");
      }
    }
    await view.read();
    view.list.querySelector(`[data-show="${CSS.escape(show.id)}"]`)?.focus();
  };
  input.addEventListener("keydown", (e) => {
    e.stopPropagation();
    if (e.key === "Enter") finish(true);
    if (e.key === "Escape") finish(false);
  });
  input.addEventListener("blur", () => finish(true));
}
