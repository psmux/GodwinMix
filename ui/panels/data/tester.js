// Try a feed, see what came back, pick a value and bind it: the three steps
// a person takes, in that order, with `feed.test` doing the looking. Nothing
// is stored until Bind is pressed.

import { el, clear } from "../../shell/dom.js";
import { bindRequest, feedIdFor, shortValue, targetsFor, testRequest } from "./model.js";

/**
 * @param {{call: Function}} client
 * @param {{sources: object[], feeds: () => string[], source?: object, bound: () => void}} opts
 */
export function tester(client, opts) {
  const field = (label, input) => el("label.col.sm", { style: { gap: "2px" } }, [el("span.dim", { text: label }), input]);
  const feedPick = el("select", { "aria-label": "Feed" });
  const address = el("input", { type: "url", placeholder: "https://example.com/news.rss", "aria-label": "Address", style: { flex: "1" } });
  const interval = el("input", { type: "number", min: 5, value: 30, "aria-label": "Every (seconds)", style: { width: "6em" } });
  const select = el("input", { placeholder: "items[].title", "aria-label": "Select" });
  const template = el("input", { placeholder: "{home} {home_score} : {away_score} {away}", "aria-label": "Template" });
  const limit = el("input", { type: "number", min: 0, placeholder: "all", "aria-label": "Limit", style: { width: "5em" } });
  const sourcePick = el("select", { "aria-label": "Source" }, opts.sources.map((s) => el("option", { value: s.id, text: s.name || s.id })));
  const path = el("input", { list: "gmx-feed-paths", value: "params.text", "aria-label": "Param" });
  const suggestions = el("datalist#gmx-feed-paths");
  const param = el("input", { placeholder: "speaker", "aria-label": "Scene parameter" });
  const toScene = el("input", { type: "checkbox", "aria-label": "Write to a scene parameter instead" });
  const status = el("p.sm.dim.tester-status", { role: "status", text: "Paste an address and press Try, or pick a feed you have." });
  const found = el("div.col.tester-found", { style: { gap: "4px" } });
  const value = el("p.sm.tester-value", { style: { margin: "0" } });

  const fillFeeds = () => {
    const keep = feedPick.value;
    clear(feedPick).append(el("option", { value: "", text: "A new feed" }), ...opts.feeds().map((id) => el("option", { value: id, text: id })));
    feedPick.value = opts.feeds().includes(keep) ? keep : "";
    address.disabled = !!feedPick.value;
  };
  const fillPaths = () => {
    const src = opts.sources.find((s) => s.id === sourcePick.value);
    const wanted = targetsFor(src);
    clear(suggestions).append(...wanted.map((t) => el("option", { value: t.path, text: t.label })));
    if (wanted.length && !wanted.some((t) => t.path === path.value)) path.value = wanted[0].path;
  };
  if (opts.source) sourcePick.value = opts.source.id;
  sourcePick.onchange = fillPaths;
  feedPick.onchange = () => (address.disabled = !!feedPick.value);
  fillFeeds();
  fillPaths();

  const form = () => ({ select: select.value, template: template.value, limit: limit.value, source: sourcePick.value, path: path.value, param: param.value, target: toScene.checked ? "scene_param" : "source" });
  const say = (text, bad) => {
    status.textContent = text;
    status.classList.toggle("bad", !!bad);
    status.classList.toggle("dim", !bad);
  };

  const show = (res) => {
    clear(found);
    found.append(el("p.sm.dim", { style: { margin: "0" }, text: `Read as ${res.format}, ${res.bytes} bytes. Top keys: ${res.keys.join(", ") || "(a list)"}. Press a path to use it.` }));
    const rows = res.paths.map((p) =>
      el("button.btn.sm.path-row", { type: "button", "data-path": p.path, style: { justifyContent: "space-between", display: "flex", width: "100%" }, onclick: () => { select.value = p.path; preview(); } }, [
        el("code", { text: p.path || "(the whole document)" }),
        el("span.dim", { text: shortValue(p.example, 50) }),
      ])
    );
    found.append(el("div.col.paths", { style: { maxHeight: "220px", overflow: "auto", gap: "2px" } }, rows));
    value.textContent = res.value !== undefined ? `Would write: ${shortValue(res.value, 200)}` : "";
  };

  const preview = () => {
    say("Fetching.");
    return client.call("feed.test", testRequest(feedPick.value, address.value.trim(), form())).then(
      (res) => { show(res); say("Here is what came back."); return res; },
      (e) => { say(e.message || String(e), true); value.textContent = ""; return null; }
    );
  };

  const bind = async () => {
    try {
      let feed = feedPick.value;
      if (!feed) {
        feed = feedIdFor(address.value.trim(), opts.feeds());
        await client.call("feed.add", { id: feed, address: address.value.trim(), interval_s: Math.max(5, Number(interval.value) || 30) });
      }
      const done = await client.call("feed.binding.add", bindRequest(feed, form()));
      say(`Bound. ${done.value !== undefined ? "On air now: " + shortValue(done.value) : "It writes when the feed is read."}`);
      opts.bound();
      fillFeeds();
      feedPick.value = feed;
      address.disabled = true;
    } catch (e) {
      say(e.message || String(e), true);
    }
  };

  const node = el("div.col.tester", { style: { gap: "8px" } }, [
    el("div.row", { style: { gap: "8px", alignItems: "end" } }, [field("Feed", feedPick), field("Address", address), field("Every (s)", interval), el("button.btn.try", { type: "button", text: "Try", onclick: preview })]),
    status,
    found,
    el("div.row", { style: { gap: "8px" } }, [field("Select", select), field("Template", template), field("Limit", limit), el("button.btn.preview", { type: "button", text: "Preview", onclick: preview, style: { alignSelf: "end" } })]),
    value,
    el("div.row", { style: { gap: "8px", alignItems: "end" } }, [field("Write to source", sourcePick), field("Param", path), suggestions, el("label.row.sm", { style: { gap: "4px", alignSelf: "end" } }, [toScene, "or scene parameter"]), field("Name", param), el("button.btn.primary.bind", { type: "button", text: "Bind", onclick: bind, style: { alignSelf: "end" } })]),
  ]);
  return { el: node, refreshFeeds: fillFeeds, preview, bind, inputs: { feedPick, address, select, template, limit, sourcePick, path, param, toScene } };
}
