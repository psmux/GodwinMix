import { ago, bindRequest, describeTarget, feedIdFor, stateLine, targetsFor, testRequest } from "../panels/data/model.js";
import { openLiveData } from "../panels/data/dialog.js";
import { connect } from "../client/index.js";

const tick = (ms = 30) => new Promise((r) => setTimeout(r, ms));

export async function liveDataTests(test, eq, ok) {
  test("a feed is named after its address's host, and never twice", () => {
    eq(feedIdFor("https://www.bbc.co.uk/news/rss.xml"), "bbc");
    eq(feedIdFor("https://bbc.co.uk/x", ["bbc"]), "bbc-2");
    eq(feedIdFor("not an address"), "feed");
    eq(feedIdFor("http://127.0.0.1:8080/x"), "feed-127");
  });
  test("a feed's state reads at a glance", () => {
    const now = Date.parse("2026-10-02T20:00:30Z");
    eq(stateLine({ state: "ok", last_fetch: "2026-10-02T20:00:18Z" }, now).text, "Read 12 s ago");
    eq(stateLine({ state: "failing", failures: 3, last_error: "the server answered 404" }, now).text, "Failing (3 tries): the server answered 404");
    eq(stateLine({ state: "failing", failures: 1 }, now).dot, "failed");
    eq(stateLine({ state: "paused", paused: true }, now).text, "Paused");
    eq(ago("2026-10-02T19:57:30Z", now), "3 min ago");
  });
  test("a ticker offers its items and a text its words", () => {
    eq(targetsFor({ type: "ticker/source" })[0].path, "params.items");
    eq(targetsFor({ type: "text/source" })[0].path, "params.text");
    eq(targetsFor({ type: "file/source" }).length, 0);
  });
  test("the form becomes a binding with only what was filled in", () => {
    const req = bindRequest("news", { select: " items[].title ", limit: "10", source: "crawl", path: "params.items", target: "source" });
    eq(JSON.stringify(req), '{"feed":"news","select":"items[].title","limit":10,"to":{"source":"crawl","path":"params.items"}}');
    eq(JSON.stringify(bindRequest("s", { select: "home", target: "scene_param", param: "home" }).to), '{"scene_param":"home"}');
    eq(JSON.stringify(testRequest("", "https://x/y", { select: "a", limit: "0" })), '{"address":"https://x/y","select":"a"}');
    eq(describeTarget({ graphic: "ograf/lower-third", field: "name" }), "ograf/lower-third, name");
  });

  await dialogWithAStub(test, eq, ok);
  await againstTheCore(test, eq, ok);
}

/** The dialog against a client that answers like a core with one failing feed. */
async function dialogWithAStub(test, eq, ok) {
  const calls = [];
  const client = {
    call: async (method, params) => {
      calls.push([method, params]);
      if (method === "source.list") return [{ id: "crawl", type: "ticker/source" }, { id: "strap", type: "text/source" }];
      if (method === "feed.list") return { feeds: [{ id: "old", kind: "polled", address: "https://old.example/x", state: "failing", failures: 2, last_error: "the server answered 404" }], bindings: [] };
      if (method === "feed.test") return { format: "rss", bytes: 900, keys: ["items", "title"], paths: [{ path: "items[].title", example: "Polls close" }, { path: "title", example: "News" }], value: params.select ? ["Polls close"] : undefined };
      if (method === "feed.binding.add") return { id: "crawl-items", value: ["Polls close"] };
      return {};
    },
  };
  const d = await openLiveData(client, { source: { id: "crawl", type: "ticker/source" } });
  test("a failing feed shows red with its reason", () => {
    const row = d.el.querySelector('[data-feed="old"]');
    ok(row && row.querySelector(".dot.failed"), d.el.innerHTML.slice(0, 300));
    ok(row.textContent.includes("Failing (2 tries): the server answered 404"), row.textContent);
  });
  test("a ticker picked from its editor is the target, with items as the param", () => {
    eq(d.form.inputs.sourcePick.value, "crawl");
    eq(d.form.inputs.path.value, "params.items");
  });
  d.form.inputs.address.value = "https://news.example.com/rss";
  d.el.querySelector("button.try").click();
  await tick();
  const pathRow = d.el.querySelector('button.path-row[data-path="items[].title"]');
  test("Try shows the paths with an example each", () => ok(pathRow, d.el.textContent.slice(0, 300)));
  pathRow && pathRow.click();
  await tick();
  test("pressing a path selects it and previews what would be written", () => {
    eq(d.form.inputs.select.value, "items[].title");
    const last = calls.filter(([m]) => m === "feed.test").pop();
    eq(last[1].select, "items[].title");
    ok(d.el.textContent.includes('Would write: ["Polls close"]'), d.el.textContent.slice(-300));
  });
  d.form.inputs.limit.value = "10";
  d.el.querySelector("button.bind").click();
  await tick();
  test("Bind adds the feed, named for its host, then the binding", () => {
    const add = calls.find(([m]) => m === "feed.add");
    ok(add && add[1].id === "news" && add[1].address === "https://news.example.com/rss" && add[1].interval_s === 30, JSON.stringify(add));
    const bind = calls.find(([m]) => m === "feed.binding.add");
    eq(JSON.stringify(bind[1]), '{"feed":"news","select":"items[].title","limit":10,"to":{"source":"crawl","path":"params.items"}}');
  });
  d.close();
}

/** A real feed, served by the core itself, bound to a real text. */
async function againstTheCore(test, eq, ok) {
  const params = new URLSearchParams(location.search);
  if (params.get("live") === "0") return;
  let client;
  try {
    client = await connect({ token: params.get("token") });
  } catch {
    return;
  }
  const address = `${location.origin}/test/live-data-feed.json`;
  await client.call("feed.remove", { id: "ui-test" }).catch(() => {});
  await client.call("source.remove", { id: "ui-test-strap" }).catch(() => {});
  await client.call("source.add", { id: "ui-test-strap", uri: "text:Waiting" });
  const tried = await client.call("feed.test", { address, select: "match", template: "{home} {home_score} : {away_score} {away}" });
  test("the core reads a JSON feed and fills a template from it", () => eq(tried.value, "Leeds 2 : 0 Hull"));
  const wrong = await client.call("feed.test", { address, select: "matches.home" }).then(() => null, (e) => e);
  test("a path that picks nothing is refused with the keys that are there", () => {
    ok(wrong && /selects nothing/.test(wrong.message) && /match/.test(wrong.message), wrong && wrong.message);
  });
  await client.call("feed.add", { id: "ui-test", address, interval_s: 3600 });
  let read = false;
  for (let i = 0; i < 100 && !read; i += 1) {
    await tick(50);
    const list = await client.call("feed.list", {});
    read = list.feeds.some((f) => f.id === "ui-test" && f.state === "ok");
  }
  test("the feed is read and says so", () => ok(read));
  await client.call("feed.binding.add", { feed: "ui-test", select: "match.home", to: { source: "ui-test-strap", path: "params.text" } });
  const strap = await client.call("source.get", { id: "ui-test-strap" });
  test("a binding writes the value into the text on the core", () => eq(strap.params && strap.params.text, "Leeds"));
  await client.call("feed.remove", { id: "ui-test" }).catch(() => {});
  await client.call("source.remove", { id: "ui-test-strap" }).catch(() => {});
}
