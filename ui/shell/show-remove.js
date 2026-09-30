// Removing a show from its tab, after a yes. The last show stays and says
// why, and so does the first, which runs from the station's own file.
//
// The page's socket is addressed to one show, and the station closes it when
// that show goes, before the answer arrives. So removing the show this page
// is on is asked over a line to another show, and the page then moves there.

import { confirmModal } from "./modal.js";
import { toast, errorToast } from "./toast.js";
import { switchTo } from "./show-actions.js";

export async function remove(view, show) {
  if (view.shows.length < 2) {
    return toast({ text: `${show.name} is the only show on this machine, so it stays. Add another show first if you want this one gone.` });
  }
  if (show.id === "main") return toast({ text: `${show.name} runs from the station's own settings file, so it cannot be removed. Stop it from its menu instead.` });
  const live = show.on_air ? " It is on air now, and stops first." : " It stops first if it is running.";
  const yes = await confirmModal(`Remove the show ${show.name}?${live} Its scenes, sources and outputs go with it.`, "Remove show");
  if (!yes) return;
  const other = view.shows.find((s) => s.id !== show.id);
  const own = show.id === view.current;
  const via = own ? (await import("../panels/routing/link.js")).showLink(view.client, other.id, view.current) : { call: (m, p) => view.client.call(m, p), close() {} };
  try {
    await via.call("show.remove", { id: show.id });
  } catch (e) {
    return errorToast(e, `Removing ${show.name}`);
  } finally {
    via.close();
  }
  toast({ text: `${show.name} removed.` });
  if (own) switchTo(other.id);
  else view.read();
}
