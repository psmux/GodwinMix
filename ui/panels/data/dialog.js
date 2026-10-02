// Live data: the feeds this show reads, and the form that adds one and binds
// a value from it to a source on air.
//
// Opened from Sources > Live data, or from a text's or a ticker's editor with
// that source picked as the target. The list is read again every two seconds
// while the dialog is open and not at all once it closes: nothing runs that
// nobody is looking at.

import { el, clear } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { feedList } from "./feed-list.js";
import { tester } from "./tester.js";

const EVERY_MS = 2000;

/**
 * @param {{call: Function, state?: {sources?: object[]}}} client
 * @param {{source?: object}} [opts]  a source to bind to, preselected
 */
export async function openLiveData(client, opts = {}) {
  let list = { feeds: [], bindings: [] };
  const listed = el("div.col.live-data-list");
  const sources = await client.call("source.list", {}).then((r) => r.sources || r || [], () => (client.state && client.state.sources) || []);
  const refresh = async () => {
    list = await client.call("feed.list", {}).catch(() => list);
    clear(listed).append(feedList(client, list, refresh));
    form.refreshFeeds();
  };
  const form = tester(client, {
    sources: Array.isArray(sources) ? sources : [],
    feeds: () => list.feeds.map((f) => f.id),
    source: opts.source,
    bound: () => refresh(),
  });
  const timer = setInterval(refresh, EVERY_MS);
  const body = el("div.col.live-data", { style: { gap: "14px" } }, [
    el("section.col", { style: { gap: "6px" } }, [el("h3", { text: "Feeds", style: { margin: "0" } }), listed]),
    el("section.col", { style: { gap: "6px" } }, [el("h3", { text: "Add a feed and bind a value", style: { margin: "0" } }), form.el]),
  ]);
  const m = modal({ title: "Live data", body, wide: true, onClose: () => clearInterval(timer) });
  await refresh();
  return { ...m, form, refresh };
}
