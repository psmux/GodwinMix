// What a Livebox user looks for on the Channels panel: every channel on one
// line, how long each destination has been sending, the bulk actions on a
// channel's push destinations, and the palette finding all of it by the words
// they already use.

import { rowsOf, rowOf, specOf } from "../panels/channels/rows-model.js";
import { rowsView } from "../panels/channels/rows.js";
import { Channels, tileState, startedAt } from "../panels/channels/model.js";
import { parsePasted, addPasted } from "../panels/channels/paste.js";
import { distinctLabels } from "../panels/channels/paste-labels.js";
import { setAll, bulkItems } from "../panels/channels/bulk.js";
import { rank } from "../shell/palette.js";
import { all } from "../shell/commands.js";
import { CHANNEL_COMMANDS } from "../panels/channels/entry.js";

const NOW = 1790700000000;
const stream = (name, over = {}) => ({
  name, state: "live", since_ms: NOW - 3723000,
  video: { codec: "h264", width: 1920, height: 1080, fps: 30, kbps: 4000 }, audio: { codec: "aac", kbps: 128 }, ...over,
});
const dest = (id, state, enabled = true) => ({ id, platform: "custom", label: id, enabled, state, since_ms: 5000, kbps: 2600 });
const ANSWER = {
  channels: [
    { id: "spare", name: "Spare", enabled: false, streams: [], destinations: [] },
    { id: "lobby", name: "Lobby", enabled: true, streams: [], destinations: [dest("yt", "waiting")] },
    {
      id: "main", name: "Main", enabled: true,
      streams: [stream("main"), stream("backup", { video: { width: 1280, height: 720, kbps: 2000 }, audio: null })],
      destinations: [dest("a", "live"), dest("b", "live"), dest("c", "live"), dest("d", "failed")],
    },
  ],
};

/** A view with a client that answers as the core does, and fails where told. */
function stubView(fail = () => null) {
  const calls = [];
  return {
    calls,
    accepted: 0,
    accept() { this.accepted += 1; },
    client: {
      async call(method, params) {
        calls.push([method, params]);
        const why = fail(params);
        if (why) throw Object.assign(new Error(why), { code: -32602 });
        return { id: params.id, destinations: [] };
      },
    },
  };
}

export async function channelRowsTests(test, eq, ok) {
  test("rows: the channel.list answer as one line a channel, live first", () => {
    const rows = rowsOf(ANSWER, NOW);
    eq(rows.map((r) => r.id), ["main", "lobby", "spare"]);
    const [main, lobby, spare] = rows;
    eq([main.state, main.status, main.spec], ["live", "Live, 2 streams", "1920×1080 30 fps 4.1 Mb/s"]);
    eq(main.began, NOW - 3723000);
    eq(main.sending, "3 of 4 sending");
    eq(main.rings.map((g) => g.state), ["live", "live", "live", "failed"]);
    eq([lobby.state, lobby.status, lobby.spec, lobby.began, lobby.sending], ["waiting", "Waiting for an encoder", "", 0, "0 of 1 sending"]);
    eq([spare.state, spare.status, spare.sending], ["off", "Switched off", "No push destinations"]);
  });

  test("rows: a stream that says less shows less, and one stream is just Live", () => {
    eq(specOf({ video: { width: 1280, height: 720, kbps: 900 } }), "1280×720 900 kb/s");
    const one = rowOf({ id: "x", enabled: true, streams: [stream("main")], destinations: [] }, NOW);
    eq(one.status, "Live");
    const tried = rowOf({ id: "y", enabled: true, streams: [], destinations: [{ id: "c", label: "Custom RTMP", enabled: true, state: "connecting", error: "nothing answered at rtmp://127.0.0.1:19999" }] }, NOW);
    eq(tried.failing, ["Custom RTMP: nothing answered at rtmp://127.0.0.1:19999"], "a first dial that failed counts as failing");
  });

  test("rows: a line a channel, and a press opens that channel", () => {
    let opened = "";
    const view = rowsView((id) => { opened = id; });
    const model = new Channels();
    model.load(ANSWER);
    view.update(model.list(), NOW);
    eq(view.node.children.length, 3);
    const main = view.node.children[0];
    ok(main.textContent.includes("3 of 4 sending"), main.textContent);
    eq(main.querySelectorAll(".chn-rring").length, 4);
    const count = main.querySelector(".chn-rsend");
    ok(count.classList.contains("bad"), "3 of 4 is red while d has failed");
    eq(count.title, "d: failed");
    ok(!view.node.children[1].querySelector(".chn-rsend").classList.contains("bad"), "a waiting one is not");
    main.click();
    eq(opened, "main");
  });

  test("a live destination says for how long, and ticks from since_ms", () => {
    const d = { enabled: true, state: "live", kbps: 2600 };
    eq(tileState(d, NOW - 3733000, NOW), "Live, 2.6 Mb/s, 1:02:13");
    eq(tileState(d, NOW - 65000, NOW), "Live, 2.6 Mb/s, 1:05");
    eq(tileState(d), "Live, 2.6 Mb/s");
    eq(tileState({ ...d, kbps: 0 }, NOW - 5000, NOW), "Live, 0:05");
    eq(tileState({ enabled: true, state: "connecting" }), "Connecting");
    eq(tileState({ enabled: true, state: "connecting", error: "nothing answered at rtmp://127.0.0.1:19999" }), "Trying again", "a first dial that failed is not still connecting");
    // A destination's since_ms is how long ago, as of the answer it came in.
    const model = new Channels();
    model.put({ id: "main", destinations: [{ id: "yt", since_ms: 5000 }] }, NOW);
    eq(startedAt(5000, model.seenAt.get("main#yt")), NOW - 5000);
  });

  test("paste: one destination a line, the key on the end or after a space", () => {
    const got = parsePasted([
      "rtmp://a.rtmp.youtube.com/live2 abcd-1234",
      "",
      "# the backup",
      "rtmps://live-api-s.facebook.com:443/rtmp/FB-1?s_bl=1",
      "  srt://10.0.0.9:9000?passphrase=secret  ",
      "rtmp://127.0.0.1:19410/other/main?psk=k1",
    ].join("\n"));
    eq(got, [
      { line: 1, platform: "custom", label: "YouTube", server: "rtmp://a.rtmp.youtube.com/live2", key: "abcd-1234" },
      { line: 4, platform: "custom", label: "Facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp/FB-1?s_bl=1" },
      { line: 5, platform: "srt", label: "10.0.0.9", server: "srt://10.0.0.9:9000?passphrase=secret" },
      { line: 6, platform: "custom", label: "127.0.0.1", server: "rtmp://127.0.0.1:19410/other/main?psk=k1" },
    ]);
  });

  test("paste: a line that cannot be a destination says why, by its number", () => {
    const got = parsePasted("http://example.com/live key\nrtmp://h/live\nyoutube key\nrtmp://h/live a b");
    eq(got.map((g) => g.line), [1, 2, 3, 4]);
    ok(got[0].error.includes("http://"), got[0].error);
    ok(got[1].error.includes("No stream key"), got[1].error);
    ok(got[2].error.includes("not an address"), got[2].error);
    ok(got[3].error.includes("one destination on each line"), got[3].error);
  });

  const view = stubView((p) => (p.server && p.server.includes("bad") ? "nothing answered at rtmp://bad" : null));
  const lines = parsePasted("rtmp://good/live/k1\nftp://x/y\nrtmp://bad/live k2\nsrt://h:9000");
  const result = await addPasted(view, "main", lines);
  test("paste: every good line is one channel.destination.add, and failures keep their line", () => {
    eq(view.calls.map(([m]) => m), ["channel.destination.add", "channel.destination.add", "channel.destination.add"]);
    eq(view.calls[0][1], { id: "main", platform: "custom", label: "good", server: "rtmp://good/live/k1" });
    eq(view.calls[1][1], { id: "main", platform: "custom", label: "bad", server: "rtmp://bad/live", key: "k2" });
    eq(view.calls[2][1].platform, "srt");
    eq(result.added, 2);
    eq(result.failed.map((f) => f.line), [2, 3]);
    eq(result.failed[1].error, "nothing answered at rtmp://bad");
  });

  test("paste: two servers on one host, or a name the channel has, get names that differ", () => {
    const two = distinctLabels(parsePasted("rtmp://127.0.0.1:19420/FakeYT/main?psk=k1\nrtmp://127.0.0.1:19420/FakeFB main?psk=k2\nrtmp://10.0.0.9/live/hall"));
    eq(two.map((l) => l.label), ["127.0.0.1/FakeYT", "127.0.0.1/FakeFB", "10.0.0.9"]);
    const again = distinctLabels(parsePasted("rtmp://a.rtmp.youtube.com/live2 k1\nrtmp://10.0.0.9/live/hall\nrtmp://10.0.0.9/live/foyer"), ["YouTube"]);
    eq(again.map((l) => l.label), ["YouTube 2", "10.0.0.9/live", "10.0.0.9/live 2"]);
    eq(distinctLabels(parsePasted("ftp://x/y"))[0].error.includes("ftp:"), true, "a bad line is left as it was");
  });

  const bulk = stubView((p) => (p.destination === "b" ? "the key was refused" : null));
  const channel = { id: "main", name: "Main", destinations: [dest("a", "off", false), dest("b", "off", false), dest("c", "live")] };
  const started = await setAll(bulk, channel, true);
  test("Start all: one channel.destination.set for each one that is off", () => {
    eq(bulk.calls.map(([, p]) => [p.destination, p.enabled]), [["a", true], ["b", true]]);
    eq(started.done, 1);
    ok(started.failed[0].includes("the key was refused"), started.failed[0]);
    const items = bulkItems(bulk, channel).filter((i) => i.label);
    eq(items.map((i) => [i.label, !!i.disabled]), [["Start all", false], ["Stop all", false], ["Paste several addresses", false]]);
    eq(bulkItems(bulk, { destinations: [] }).filter((i) => i.label).map((i) => !!i.disabled), [true, true, false]);
  });

  test("the palette finds Channels by the words Livebox and encoders use", () => {
    const top = (q) => rank(all(), q)[0]?.id;
    for (const q of ["push destination", "push", "restream", "stream key", "stream url", "channel dashboard"]) eq(top(q), "channel.open", q);
    const bulky = rank(all(), "bulk").slice(0, 2).map((c) => c.id).sort();
    eq(bulky, ["channel.paste", "channel.rows"]);
    ok(CHANNEL_COMMANDS.every((c) => c.group === "Channels"));
    eq(rank(CHANNEL_COMMANDS, "add")[0].id, "channel.add");
  });
}
