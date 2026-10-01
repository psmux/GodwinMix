// The monitoring wall against wall-stub.js: the arithmetic, the list parser,
// and 200 shows with only the rows on screen drawn and read. The keys and
// clicks are wall-act.js, bulk add and a show's detail wall-dialogs.js. No
// core needed; the shapes are dev/plans/wave4-contract.md.

import { wallStub, headend } from "./wall-stub.js";
import { wait, until, sent, openOn } from "./wall-help.js";
import { actTests } from "./wall-act.js";
import { bulkTests, detailTests } from "./wall-dialogs.js";

export async function wallTests(test, eq, ok) {
  await modelTests(test, eq, ok);
  await parseTests(test, eq, ok);
  await bigTests(test, eq, ok);
  await actTests(test, eq, ok);
  await bulkTests(test, eq, ok);
  await detailTests(test, eq, ok);
}

async function modelTests(test, eq, ok) {
  const m = await import("../panels/wall/model.js");
  test("an input's transport is read from its address", () => {
    eq(["udp://@239.1.1.1:5000", "udp://10.0.0.1:5000", "srt://h:9000", "rtmp://h/app/k", "https://cdn/x.m3u8", "channel:main/cam"].map(m.transport), ["Multicast", "UDP", "SRT", "RTMP", "HLS", "Channel"]);
  });
  test("numbers in the fewest words", () => {
    eq([m.kbps(850), m.kbps(4400), m.kbps(12000), m.kbps(1200000)], ["850 kb/s", "4.4 Mb/s", "12 Mb/s", "1.20 Gb/s"]);
    eq([m.age(0, 45000), m.age(0, 600000), m.age(0, 7500000)], ["45 s", "10 min", "2 h 5 min"]);
  });
  const c = await import("../panels/wall/cells.js");
  test("Load says what a show does: a show that mixes never reads copy only", () => {
    const mix = { id: "main", compositing: true };
    const direct = { id: "news", compositing: false };
    const copyOut = { id: "o", encoder: null, cpu_millicores: 0 };
    eq(c.loadText(mix, { work: "mix", cpu_millicores: 350, outputs: [] }), "35% core");
    eq(c.loadText(mix, { work: "mix", outputs: [] }), "mixing", "not measured yet");
    eq(c.loadText(mix, { outputs: [] }), "mixing", "a core from before work: compositing decides");
    eq(c.loadText(direct, { work: "copy", cpu_millicores: 0, outputs: [copyOut] }), "copy only");
    eq(c.loadText(direct, { work: "transcode", cpu_millicores: 1250, outputs: [] }), "1.3 cores");
    eq(c.loadText(direct, { outputs: [{ id: "o", encoder: "x264", cpu_millicores: 90 }] }), "9% core");
    eq(c.loadText(direct, null), "");
  });
  test("the header's CPU is what the station measures of its processes, when it says", () => {
    const busy = { cores: 10, used_millicores: 20, room_millicores: 6000, measured_millicores: 5200 };
    eq(m.summary([], new Map(), { cpu: busy }).cpu, 52, "the direct host and the shows counted");
    eq(m.summary([], new Map(), { cpu: { cores: 10, used_millicores: 2500, room_millicores: 6000 } }).cpu, 25, "a core that does not measure");
  });
  const shows = headend(80, 1e9);
  const rows = (o) => m.rows(shows, new Map(), o);
  test("shows in alarm come first, then warnings, then the rest by name", () => {
    const r = rows({});
    eq(r.slice(0, 6).map((x) => m.healthOf(x.show).state), ["alarm", "alarm", "alarm", "alarm", "alarm", "alarm"]);
    eq(m.healthOf(r[6].show).state, "warning");
  });
  test("a filter matches names, addresses and alarm words; the alarm choice narrows by kind", () => {
    eq(rows({ text: "kids", sort: "name" }).map((x) => x.show.name), ["Kids 1", "Kids 2", "Kids 3", "Kids 4"]);
    eq(rows({ text: "frozen" }).map((x) => x.show.name), ["Science 1"]);
    eq(rows({ alarm: "black" }).map((x) => x.show.name), ["Kids 1"]);
    eq(rows({ alarm: "any" }).length, 9);
  });
  test("grouped by state, each group under its band", () => {
    const g = rows({ group: true }).filter((x) => x.kind === "group");
    eq(g.map((x) => [x.label, x.count]), [["In alarm", 6], ["Warning", 3], ["Running", 69], ["Off", 2]]);
  });
}

async function parseTests(test, eq, ok) {
  const p = await import("../panels/wall/bulk-parse.js");
  test("one address per line is one direct show each, named from the address", () => {
    const r = p.parse("udp://@239.1.1.1:5000\n\n# a comment\n239.1.1.2:5000\nsrt://feed.example:9000/news");
    eq(r.map((x) => [x.name, x.input]), [["239.1.1.1:5000", "udp://@239.1.1.1:5000"], ["239.1.1.2:5000", "udp://@239.1.1.2:5000"], ["news", "srt://feed.example:9000/news"]]);
  });
  test("CSV with a header in any order, quotes honoured, and a spreadsheet's tabs", () => {
    const r = p.parse('output,name,input\nudp://10.0.0.5:6000,"News, late",udp://@239.1.1.1:5000');
    eq(r[0], { name: "News, late", input: "udp://@239.1.1.1:5000", program: "", output: "udp://10.0.0.5:6000", format: "" });
    const t = p.parse("Sport\tudp://@239.1.1.2:5000\t2\tsrt://x:1\tyoutube-720p30");
    eq([t[0].name, t[0].program, t[0].format], ["Sport", "2", "youtube-720p30"]);
  });
  test("a row becomes a ShowAdd: direct, its program, copy or a preset", () => {
    eq(p.toShow({ name: "News", input: "udp://@239.1.1.1:5000", program: "3", output: "udp://a:1; srt://b:2", format: "copy" }), {
      name: "News", compositing: false, input: { uri: "udp://@239.1.1.1:5000", program: 3 }, outputs: [{ uri: "udp://a:1", rendition: null }, { uri: "srt://b:2", rendition: null }],
    });
    eq(p.toShow({ name: "X", input: "srt://h:1", output: "srt://o:2", format: "youtube-720p30" }).outputs[0].rendition, { preset: "youtube-720p30" });
  });
}

async function bigTests(test, eq, ok) {
  const stub = wallStub({ n: 200 });
  const view = await openOn(stub);
  const drawn = () => [...view.root.querySelectorAll(".wl-row")].map((r) => r.dataset.id);
  const lastAsked = () => stub.statsAsked.at(-1) || [];
  const order = view.items.map((it) => it.show.id);
  test("200 shows: only the rows in view and a margin are drawn", () => {
    ok(drawn().length > 5 && drawn().length < 40, `${drawn().length} rows drawn of 200`);
    ok(!drawn().includes(order.at(-1)), "the last show is not drawn");
  });
  test("show.stats asks for the rows on screen only, and pictures likewise", () => {
    const ids = lastAsked();
    ok(ids.length > 3 && ids.length <= drawn().length, `${ids.length} asked, ${drawn().length} drawn`);
    ok(ids.every((id) => order.indexOf(id) < 20), "every id asked is near the top");
    const pics = new Set(stub.thumbAsked);
    ok(pics.size <= ids.length, `${pics.size} pictures asked for`);
    ok(!pics.has(order.at(-1)), "no picture for a show off screen");
  });
  test("the header counts every show, not only those drawn", () => {
    const t = view.root.querySelector(".wl-summary").textContent;
    ok(/200 shows/.test(t) && /6 in alarm/.test(t) && /CPU/.test(t), t);
  });
  const before = sent(stub, "show.stats").length;
  await wait(2150);
  test("one show.stats a second, whatever the number of shows", () => {
    const n = sent(stub, "show.stats").length - before;
    ok(n >= 1 && n <= 3, `${n} calls in two seconds`);
  });

  const scroll = view.root.querySelector(".wl-scroll");
  scroll.scrollTop = scroll.scrollHeight;
  scroll.dispatchEvent(new Event("scroll"));
  await until(() => lastAsked().includes(order.at(-1)));
  test("scrolling to the bottom reads the shows that came into view and lets go of the top", () => {
    ok(lastAsked().includes(order.at(-1)), "the last show is read");
    ok(!lastAsked().includes(order[0]), "the first is not");
    ok(drawn().includes(order.at(-1)) && !drawn().includes(order[0]), "and the drawn rows moved with it");
  });
  scroll.scrollTop = 0;
  scroll.dispatchEvent(new Event("scroll"));
  await wait(60);

  stub.event("show.health", { id: "arts-10", health: { state: "alarm", alarms: [{ kind: "black", since_ms: Date.now() - 4000 }] } });
  await wait(80);
  test("a show.health event moves the show into alarm and the count with it", () => {
    ok(/7 in alarm/.test(view.root.querySelector(".wl-summary").textContent), "seven now");
    ok(view.items.slice(0, 7).some((it) => it.show.id === "arts-10"), "and up among the alarms");
  });

  view.root.querySelector('[data-top="tiles"]').click();
  await until(() => view.root.querySelector(".wl-tile"));
  test("the tile view draws only the lines of tiles in view", () => {
    const n = view.root.querySelectorAll(".wl-tile").length;
    ok(n > 3 && n < 80, `${n} tiles drawn`);
  });
  view.root.querySelector('[data-top="rows"]').click();
  await wait(60);

  const calls = sent(stub, "show.stats").length;
  view.close();
  await wait(1300);
  test("closing stops every read and gives the events back", () => {
    ok(!document.querySelector(".wall"), "gone");
    eq(sent(stub, "show.stats").length, calls, "no show.stats after closing");
    ok(!stub.patterns.has("show.*"), [...stub.patterns.keys()].join(","));
  });
}
