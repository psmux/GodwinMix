// What the Live data dialog works out without touching the page: an id for
// a feed from its address, how long ago something was, a feed's state in a
// line, where a binding writes, and the request that makes one.

/** A slug for a feed, from the address's host, not one already taken. */
export function feedIdFor(address, taken = []) {
  let base = "feed";
  try {
    const host = new URL(address).hostname.replace(/^www\./, "");
    base = host.split(".")[0] || base;
  } catch {}
  base = base.toLowerCase().replace(/[^a-z0-9-]+/g, "-").replace(/^-+|-+$/g, "") || "feed";
  if (!/^[a-z]/.test(base)) base = "feed-" + base;
  let id = base;
  for (let n = 2; taken.includes(id); n += 1) id = `${base}-${n}`;
  return id;
}

/** "just now", "12 s ago", "3 min ago", "2 h ago". */
export function ago(iso, now = Date.now()) {
  const t = Date.parse(iso || "");
  if (Number.isNaN(t)) return "";
  const s = Math.max(0, Math.round((now - t) / 1000));
  if (s < 3) return "just now";
  if (s < 90) return `${s} s ago`;
  if (s < 5400) return `${Math.round(s / 60)} min ago`;
  return `${Math.round(s / 3600)} h ago`;
}

/** A feed's state as a dot class and a line a person reads at a glance. */
export function stateLine(feed, now = Date.now()) {
  if (feed.paused || feed.state === "paused") return { dot: "", text: "Paused" };
  if (feed.state === "failing") {
    const tries = feed.failures > 1 ? ` (${feed.failures} tries)` : "";
    return { dot: "failed", text: `Failing${tries}: ${feed.last_error || "no reason given"}` };
  }
  if (feed.state === "ok") {
    const read = ago(feed.last_fetch, now);
    const changed = feed.last_change ? `, changed ${ago(feed.last_change, now)}` : "";
    return { dot: "live", text: `Read ${read}${changed}` };
  }
  return { dot: "connecting", text: feed.kind === "polled" ? "Fetching" : "Connecting" };
}

/** The params a source of this kind takes words or a list in. */
export function targetsFor(source) {
  const type = (source && source.type) || "";
  if (type === "ticker/source") return [{ label: "Items, one each", path: "params.items" }, { label: "Words", path: "params.text" }];
  if (type === "text/source") return [{ label: "Words", path: "params.text" }];
  return [];
}

/** Where a binding writes, in a few words. */
export function describeTarget(to) {
  if (!to) return "";
  if (to.source) return `${to.source}, ${to.path}`;
  if (to.graphic) return `${to.graphic}${to.item ? ` (${to.item})` : ""}, ${to.field}`;
  if (to.scene_param) return `scene parameter {{${to.scene_param}}}`;
  return JSON.stringify(to);
}

/** A value cut to fit a line. */
export function shortValue(v, max = 80) {
  if (v === undefined || v === null) return "";
  const s = typeof v === "string" ? v : JSON.stringify(v);
  return s.length > max ? s.slice(0, max - 3) + "..." : s;
}

/** `feed.binding.add` from what the form holds. Empty fields are left out. */
export function bindRequest(feed, form) {
  const req = { feed, select: (form.select || "").trim() };
  if (form.template) req.template = form.template;
  if (Number(form.limit) > 0) req.limit = Number(form.limit);
  if (form.join) req.join = form.join;
  if (form.target === "scene_param") req.to = { scene_param: (form.param || "").trim() };
  else req.to = { source: form.source, path: (form.path || "params.text").trim() };
  return req;
}

/** `feed.test` with the selection the form holds, for a feed or an address. */
export function testRequest(feed, address, form) {
  const req = feed ? { id: feed } : { address };
  const select = (form.select || "").trim();
  if (select) req.select = select;
  if (form.template) req.template = form.template;
  if (Number(form.limit) > 0) req.limit = Number(form.limit);
  if (form.join) req.join = form.join;
  return req;
}
