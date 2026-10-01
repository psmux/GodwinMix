// The monitoring wall, bulk add and a direct show's detail against
// wall-stub.js: the arithmetic, 200 shows with only the rows on screen drawn
// and read, sorting, filtering, grouping, the keys, health events, the
// compositing switch, closing, and the two dialogs. No core needed; the
// shapes are dev/plans/wave4-contract.md.

import { wallStub, headend } from "./wall-stub.js";

const wait = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 3000) => { const end = Date.now() + ms; while (!fn() && Date.now() < end) await wait(20); return fn(); };
const sent = (stub, method) => stub.calls.filter((c) => c.method === method);
const key = (node, k) => node.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));
const dialogs = () => [...document.querySelectorAll(".dialog")];
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeDialogs = () => { for (const d of dialogs()) d.closest(".scrim")?.remove(); };

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

async function openOn(stub) {
  try { localStorage.removeItem("gmx.wall"); } catch { /* fine */ }
  const { toggleWall } = await import("../panels/wall/view.js");
  const view = toggleWall(stub);
  await until(() => view.root.querySelector(".wl-row"));
  await until(() => stub.statsAsked.length);
  await wait(80);
  return view;
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

async function actTests(test, eq, ok) {
  const stub = wallStub({ n: 40 });
  const view = await openOn(stub);
  const rows = () => [...view.root.querySelectorAll(".wl-row")];
  view.root.querySelector('[data-sort="name"]').click();
  await wait(40);
  test("a click on a header sorts by it, and again the other way", () => eq(rows()[0].dataset.id, "arts-1"));
  view.root.querySelector('[data-sort="name"]').click();
  await wait(40);
  test("the second click reverses it", () => eq(rows()[0].dataset.id, "weather-2"));

  const filter = view.root.querySelector(".wl-filter");
  filter.value = "multicast kids";
  filter.dispatchEvent(new Event("input"));
  await wait(40);
  test("the filter keeps what matches every word", () => eq(rows().map((r) => r.dataset.id).sort(), ["kids-1", "kids-2"]));
  filter.value = "";
  filter.dispatchEvent(new Event("input"));
  view.root.querySelector('[data-sort="alarms"]').click();
  view.root.querySelector('[data-top="group"]').click();
  await wait(40);
  test("group by state puts a band over each group", () => eq([...view.root.querySelectorAll(".wl-band > span:first-child")].map((b) => b.textContent).slice(0, 2), ["In alarm", "Warning"]));
  view.root.querySelector('[data-top="group"]').click();
  await wait(40);

  const scroll = view.root.querySelector(".wl-scroll");
  scroll.focus();
  key(scroll, "ArrowDown");
  key(scroll, "ArrowDown");
  await wait(40);
  const second = view.items[1].show.id;
  test("arrow keys move the cursor down the rows", () => {
    eq(view.cursor, second);
    ok(view.root.querySelector(`#wl-${second}`).classList.contains("cursor"), "drawn as the cursor");
  });
  const flashing = () => view.root.querySelectorAll(`#wl-${second} .wl-alarm.flash`).length;
  const was = flashing();
  key(scroll, "a");
  await wait(40);
  test("A acknowledges the row's alarms: they stop flashing and stay listed", () => {
    ok(was > 0, "it was flashing");
    eq(flashing(), 0);
    ok(view.root.querySelector(`#wl-${second} .wl-alarm`), "still listed");
  });

  key(scroll, "Enter");
  await until(() => dialogs().length);
  test("Enter on a direct show opens its detail", () => eq(dialogs().at(-1).querySelector("h2").textContent, stub.shows.find((s) => s.id === second).name));
  closeDialogs();

  const id = view.items[3].show.id;
  view.root.querySelector(`#wl-${id} [data-act=mix]`).click();
  await until(() => sent(stub, "show.set").length);
  test("the switch on a row sends show.set with compositing", () => eq(sent(stub, "show.set")[0].params, { id, compositing: true }));
  const show = stub.shows.find((s) => s.id === id);
  show.scenes_in_use = 3;
  await wait(60);
  view.root.querySelector(`#wl-${id} [data-act=mix]`).click();
  await until(() => sent(stub, "show.set").length > 1);
  await wait(60);
  test("a refusal puts the switch back and says why", () => {
    eq(view.root.querySelector(`#wl-${id} [data-act=mix]`).getAttribute("aria-checked"), "true");
    ok([...document.querySelectorAll(".toast")].some((t) => /cannot go direct/.test(t.textContent)), "said");
  });
  view.close();

  const bare = wallStub({ noShows: true, n: 1 });
  const { toggleWall } = await import("../panels/wall/view.js");
  const lone = toggleWall(bare);
  await until(() => !lone.root.querySelector(".wl-empty").hidden && lone.data.loaded);
  test("a station without a show list says so plainly", () => ok(/no list of shows/.test(lone.root.querySelector(".wl-empty").textContent), lone.root.querySelector(".wl-empty").textContent));
  lone.close();

  const older = wallStub({ n: 3 });
  const inner = older.call;
  older.call = async (m, p) => {
    if (m !== "show.stats") return inner(m, p);
    older.calls.push({ method: m, params: p });
    throw Object.assign(new Error("there is no method 'show.stats'."), { code: -32004, data: { kind: "method", id: m } });
  };
  const wave3 = toggleWall(older);
  await until(() => wave3.root.querySelector(".wl-row"));
  const pics = older.thumbAsked.length;
  await wait(2300);
  test("a station from before show.stats is asked once, and for no pictures after", () => {
    eq(sent(older, "show.stats").length, 1);
    eq(older.thumbAsked.length, pics);
  });
  wave3.close();
}

async function bulkTests(test, eq, ok) {
  const stub = wallStub({ n: 5 });
  const { bulkAdd } = await import("../panels/wall/bulk.js");
  let done = 0;
  const b = bulkAdd(stub, { onDone: () => (done += 1) });
  b.dialog.el.querySelector(".wl-ghost").click();
  await wait(40);
  test("the example is three multicast feeds with UDP outputs, in the table to fix", () => {
    eq(b.rows.map((r) => r.input), ["udp://@239.1.1.1:5000", "udp://@239.1.1.2:5000", "udp://@239.1.1.3:5000"]);
    eq(b.dialog.el.querySelectorAll(".wl-brow").length, 3);
  });
  const cell = b.dialog.el.querySelector('.wl-brow[data-row="2"] [data-field="name"]');
  cell.value = "News 1";
  cell.dispatchEvent(new Event("input"));
  button(b.dialog.el, "Check").click();
  await until(() => b.dialog.el.querySelector(".wl-plan"));
  test("Check is a dry run of the whole list; a refused row says why under itself", () => {
    const p = sent(stub, "show.add_many")[0].params;
    eq([p.dry_run, p.shows.length, p.shows[2].name, p.shows[0].compositing], [true, 3, "News 1", false]);
    ok(b.dialog.el.querySelector(".wl-brow.refused"), "the row is marked");
    ok(/already a show called News 1/.test(b.dialog.el.querySelector(".wl-bwhy").textContent), "and says why");
    eq(button(b.dialog.el, "Add the 2 that are ready") ? "yes" : "no", "yes");
  });
  button(b.dialog.el, "Add the 2 that are ready").click();
  await until(() => sent(stub, "show.add_many").length > 1);
  await wait(60);
  test("Add applies it; what was refused stays in the table to fix", () => {
    eq(sent(stub, "show.add_many")[1].params.dry_run, false);
    eq(stub.shows.length, 7);
    eq(b.rows.map((r) => r.name), ["News 1"]);
    eq(done, 1);
  });
  closeDialogs();
}

async function detailTests(test, eq, ok) {
  const stub = wallStub({ n: 5 });
  const { showDetail } = await import("../panels/wall/detail.js");
  const show = stub.shows[1];
  const d = showDetail(stub, show);
  const box = d.dialog.el;
  await wait(80);
  const backup = box.querySelector('[aria-label="Backup input address"]');
  backup.value = "srt://backup:9000";
  button(box, "Save input").click();
  await until(() => sent(stub, "show.set").length);
  test("Save input sends the input with its backup", () => eq(sent(stub, "show.set")[0].params, { id: show.id, input: { uri: show.input.uri, backup: { uri: "srt://backup:9000" } } }));

  button(box, "Turn off").click();
  await until(() => sent(stub, "show.output.set").length);
  test("an output turns off with show.output.set", () => eq(sent(stub, "show.output.set")[0].params, { show: show.id, id: "udp-out", enabled: false }));

  button(box, "Add output").click();
  await until(() => box.querySelector(".wl-dbox .rnd-card"));
  await wait(80);
  box.querySelector('.wl-dbox input[type=text]').value = "srt://cdn.example:7000";
  button(box.querySelector(".wl-dbox"), "Add output").click();
  await until(() => sent(stub, "show.output.add").length);
  test("Add output sends the address and the format chosen, copy by default", () => eq(sent(stub, "show.output.add")[0].params, { show: show.id, uri: "srt://cdn.example:7000", rendition: null }));

  box.querySelector('[aria-label="Black after, in seconds"]').value = "3";
  button(box, "Save alarms").click();
  await until(() => sent(stub, "show.set").length > 1);
  test("alarm thresholds go with show.set, in milliseconds", () => eq(sent(stub, "show.set")[1].params.alarms.black_ms, 3000));
  d.close();
}
