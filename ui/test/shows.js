// Show tabs and the routing view against shows-stub.js: the tab row in its
// three shapes, switching, rename, the menu, remove, New show, and the
// routing model and view with only what is on screen fetched. No core and
// no station needed; the shapes are dev/plans/wave3-contract.md.

import { showStub, sundayShows, sundayChannels } from "./shows-stub.js";

const wait = (ms = 40) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 2000) => { const end = Date.now() + ms; while (!fn() && Date.now() < end) await wait(20); return fn(); };
const dialogs = () => [...document.querySelectorAll(".dialog")];
const button = (root, text) => [...root.querySelectorAll("button")].find((b) => b.textContent.trim() === text);
const closeAll = () => { for (const d of dialogs()) d.closest(".scrim")?.remove(); document.querySelector(".menu")?.remove(); };
const sent = (stub, method) => stub.calls.filter((c) => c.method === method);
const key = (node, k) => node.dispatchEvent(new KeyboardEvent("keydown", { key: k, bubbles: true }));

async function row(stub) {
  const { showTabs } = await import("../shell/show-tabs.js");
  const node = showTabs(stub);
  document.body.append(node);
  await wait(60);
  return node;
}

export async function showTests(test, eq, ok) {
  await tabTests(test, eq, ok);
  await actionTests(test, eq, ok);
  await modelTests(test, eq, ok);
  await viewTests(test, eq, ok);
}

async function tabTests(test, eq, ok) {
  const one = showStub({ shows: [{ id: "main", name: "Sunday service", state: "running", on_air: null }] });
  const calm = await row(one);
  test("one show is its name and a +, drawn as plainly as a label", () => {
    ok(!calm.hidden && calm.classList.contains("one"), "shown, as one");
    eq([...calm.querySelectorAll(".showtab")].map((t) => t.textContent), ["Sunday service"]);
    ok(calm.querySelector(".shows-add"), "a + for a new show");
  });
  calm.remove();

  const stub = showStub();
  const three = await row(stub);
  const tabs = () => [...three.querySelectorAll(".showtab")];
  test("three shows are three tabs, this one selected, on air and stopped said", () => {
    eq(tabs().map((t) => t.querySelector(".showtab-name").textContent), ["Sunday service", "Kids room", "Overnight loop"]);
    eq(tabs().map((t) => t.getAttribute("aria-selected")), ["true", "false", "false"]);
    eq(tabs().map((t) => t.tabIndex), [0, -1, -1], "one tab stop, on the selected tab");
    ok(tabs()[0].querySelector(".dot.onair"), "a red dot on the show that is on air");
    eq(tabs()[2].querySelector(".showtab-state").textContent, "stopped");
    ok(stub.patterns.has("show.*"), "it asks for show events while it is on the page");
  });

  stub.shows[1].name = "Kids hall";
  stub.emit("event", { name: "show.changed", params: { show: stub.shows[1] } });
  await wait(60);
  test("a show.changed event redraws the row", () => eq(tabs()[1].textContent, "Kids hall"));

  tabs()[0].focus();
  key(tabs()[0], "ArrowRight");
  await wait(60);
  test("arrow keys move along the tabs", () => ok(document.activeElement === tabs()[1], "the second tab has focus"));
  three.remove();

  const old = showStub({ noShows: true });
  const none = await row(old);
  test("a core without shows gets no row at all", () => ok(!none.isConnected && !old.patterns.has("show.*"), "removed, and its events given back"));
}

async function actionTests(test, eq, ok) {
  const actions = await import("../shell/show-actions.js");
  const went = [];
  actions.nav.go = (url) => went.push(url);
  const stub = showStub();
  const node = await row(stub);
  const tab = (id) => node.querySelector(`[data-show="${id}"]`);

  tab("kids").click();
  await until(() => went.length);
  test("a click on another show loads the page on that show", () => ok(/[?&]show=kids\b/.test(went[0] || ""), went[0]));

  key(tab("main"), "F2");
  await until(() => node.querySelector(".showtab-input"));
  const input = node.querySelector(".showtab-input");
  input.value = "Morning service";
  key(input, "Enter");
  await until(() => sent(stub, "show.rename").length);
  test("F2 renames in place, and Enter sends it", () => eq(sent(stub, "show.rename")[0].params, { id: "main", name: "Morning service" }));

  tab("main").click();
  await until(() => document.querySelector(".menu"));
  const menu = document.querySelector(".menu");
  test("a click on this show's own tab opens its menu", () => {
    const items = [...menu.querySelectorAll("button")].map((b) => b.firstChild.textContent);
    eq(items, ["Rename", "Stop show", "Remove show…"]);
  });
  closeAll();

  tab("loop").dispatchEvent(new MouseEvent("contextmenu", { bubbles: true, clientX: 300, clientY: 40 }));
  await until(() => document.querySelector(".menu"));
  button(document.querySelector(".menu"), "Start show").click();
  await until(() => sent(stub, "show.start").length);
  test("a stopped show starts from its menu", () => eq(sent(stub, "show.start")[0].params, { id: "loop" }));

  key(tab("kids"), "Delete");
  await until(() => dialogs().length);
  button(dialogs().at(-1), "Remove show").click();
  await until(() => sent(stub, "show.remove").length);
  test("Delete asks, then removes", () => eq(sent(stub, "show.remove")[0].params, { id: "kids" }));
  node.remove();

  const lone = showStub({ shows: [sundayShows()[0]] });
  const single = await row(lone);
  key(single.querySelector(".showtab"), "Delete");
  await wait(120);
  const said = [...document.querySelectorAll(".toast")].map((t) => t.textContent).join(" ");
  test("the last show is not removed, and the page says why", () => {
    eq(sent(lone, "show.remove").length, 0);
    ok(/only show on this machine/.test(said), said);
  });
  single.remove();

  const onKids = showStub({ current: "kids" });
  const kidsRow = await row(onKids);
  went.length = 0;
  key(kidsRow.querySelector('[data-show="kids"]'), "Delete");
  await until(() => dialogs().length);
  button(dialogs().at(-1), "Remove show").click();
  await until(() => went.length);
  test("removing the show the page is on asks over another show's line, then moves there", () => {
    eq(onKids.links.map((l) => [l.id, l.closed]), [["main", true]]);
    eq(onKids.links[0].calls.map((c) => c.method), ["show.remove"]);
    ok(/[?&]show=main\b/.test(went[0] || ""), went[0]);
  });
  kidsRow.remove();
  key(tab("main"), "Delete");
  await wait(120);
  test("the first show says why it cannot be removed", () => ok([...document.querySelectorAll(".toast")].some((t) => /station's own settings file/.test(t.textContent)), "said"));

  const file = await import("../shell/show-file.js");
  went.length = 0;
  await file.newShow(stub);
  const d = dialogs().at(-1);
  test("New show offers empty, a copy of this show, and a project file", () => {
    eq([...d.querySelectorAll(".show-from strong")].map((s) => s.textContent), ["Empty", "A copy of Morning service", "From a project file"]);
  });
  d.querySelector('input[type=text]').value = "Rehearsal";
  d.querySelector('input[value="main"]').click();
  button(d, "Make show").click();
  await until(() => went.length, 2000);
  test("Make show sends show.add and moves the page to the new show", () => {
    eq(sent(stub, "show.add")[0].params, { name: "Rehearsal", from: "main" });
    ok(/[?&]show=show-1\b/.test(went[0] || ""), went[0]);
  });
  closeAll();
  actions.nav.go = (url) => location.assign(url);

  const { withShow } = await import("../client/transport-rpc.js");
  const here = location.href;
  history.replaceState(null, "", "?show=kids");
  const u = withShow(new URL("ws://h/rpc?token=t"));
  history.replaceState(null, "", here);
  test("the socket and the REST paths carry the page's show", () => eq(u.searchParams.get("show"), "kids"));
}

async function modelTests(test, eq, ok) {
  const m = await import("../panels/routing/model.js");
  const stub = showStub();
  const { sunday } = await sundayChannels(stub);
  const channels = (await stub.call("channel.list", {})).channels;
  const detail = stub.detail;
  const list = m.groups(channels, stub.shows.map((s) => ({ ...s, detail: s.id === "loop" ? null : detail[s.id] })));
  const plans = { [`c:${sunday}`]: stub.plans[`channel:${sunday}`], "s:main": detail.main.plan };
  const g = (k) => list.find((x) => x.key === k);
  const at = (gk, row, col) => { const x = g(gk); return m.cell(x, x.rows.find((r) => r.id === row), x, x.cols.find((c) => c.id === col), plans); };

  test("inputs and outputs are grouped by channel and by show", () => {
    eq(list.map((x) => x.key), ["c:sunday-service", "c:youth-room", "c:radio", "s:main", "s:kids", "s:loop"]);
    eq(g("c:sunday-service").rows.map((r) => r.id), ["main", "backup"]);
    eq(g("c:radio").rows.map((r) => r.label), ["Nothing publishing"]);
    eq(g("s:main").rows.map((r) => r.id), ["programme", "cam-wide", "cam-pulpit", "lyrics", "sunday-service-main"]);
    ok(!g("s:loop").loaded, "a show not yet read is drawn without its outputs");
  });

  test("each cell says copy, or the format and where it is encoded, with the cost", () => {
    eq(at("c:sunday-service", "main", "youtube"), { kind: "route", format: "Copy", where: "no encode", cost: 0, tone: "copy" });
    const fb = at("c:sunday-service", "main", "facebook");
    eq([fb.format, fb.where, fb.tone], ["Facebook 720p30", "GPU, VideoToolbox", "gpu"]);
    eq(m.costWords(at("c:sunday-service", "main", "twitch").cost), "1.2 cores");
    eq(at("c:sunday-service", "backup", "backup-server").format, "Copy", "a destination that names its stream");
    eq(at("s:main", "programme", "youtube").format, "Programme encode");
    eq(at("s:kids", "programme", "hall-screen").format, "1280×720 · 2.5 Mb/s");
  });

  test("an empty cell offers the stream; a source on air reaches outputs through the programme", () => {
    eq(at("c:sunday-service", "backup", "youtube").kind, "offer");
    eq(at("s:main", "cam-wide", "youtube").kind, "via");
    eq(at("s:main", "cam-pulpit", "youtube").kind, "none");
    const x = g("c:sunday-service");
    eq(m.cell(x, x.rows[0], g("s:main"), g("s:main").cols[0], plans).kind, "none", "a channel stream is not a show's output");
  });

  test("`*` reads the stream live longest", () => eq(m.streamOf(g("c:sunday-service"), { stream: "*" }), "main"));

  test("a filter keeps what matches, with everything it could read", () => {
    const f = m.filtered(list, "twitch");
    eq(f.map((x) => x.key), ["c:sunday-service"]);
    eq(f[0].cols.map((c) => c.id), ["twitch"]);
    eq(f[0].rows.map((r) => r.id), ["main", "backup"]);
    eq(m.filtered(list, "kids").map((x) => x.key), ["s:kids"]);
  });

  test("names short enough for a cell", () => {
    eq(m.presetWords("abr-ladder-4"), "Ladder 4");
    eq(m.encoderName("h264-software-x264"), "x264");
    eq(m.costWords(250), "25% of a core");
  });
}

async function viewTests(test, eq, ok) {
  const { toggleRouting } = await import("../panels/routing/view.js");
  const many = [...sundayShows(), ...Array.from({ length: 17 }, (_, i) => ({ id: `room-${i + 1}`, name: `Room ${i + 1}`, state: "running", on_air: null }))];
  const detail = { ...(await import("./shows-stub.js")).sundayDetail() };
  for (const s of many.slice(3)) detail[s.id] = { sources: [{ id: `${s.id}-cam`, name: "Camera" }], tally: {}, outputs: [{ id: `${s.id}-out`, type: "rtmp/output", uri_host: "rtmp://x/", state: "live" }], plan: { nodes: [] } };
  const stub = showStub({ shows: many, detail });
  await sundayChannels(stub);
  const view = toggleRouting(stub);
  view.data.letGo = 30;
  await until(() => view.root.querySelector(".rt-grid"));
  await wait(300);
  const linked = () => stub.links.filter((l) => !l.closed).map((l) => l.id);
  test("the view opens with every group, and reads only the shows on screen", () => {
    eq(view.root.querySelectorAll("tbody.rt-group").length, 23);
    ok(linked().length < 20, `${linked().length} shows read of 20`);
    ok(!linked().includes("room-17"), "the last show, off screen, is not read");
    ok(!linked().includes("main"), "this show is read on the page's own socket");
  });

  const scroll = view.root.querySelector(".rt-scroll");
  // A group grows as its sources arrive and pushes the ones below it down,
  // so the bottom is chased until the last show stays in view: read, and
  // the grid no taller for a quarter of a second. Stopping at the first read
  // left the shows just above it still loading on a slower machine, and
  // their rows pushed the last one out of view and let it go again.
  let [height, since] = [-1, Date.now()];
  await until(() => {
    scroll.scrollTop = scroll.scrollHeight;
    if (scroll.scrollHeight !== height) [height, since] = [scroll.scrollHeight, Date.now()];
    return linked().includes("room-17") && Date.now() - since >= 250;
  }, 6000);
  await wait(200);
  test("scrolling down reads what came into view and lets go of what left", () => {
    ok(linked().includes("room-17"), "the last show is read now");
    ok(stub.links.some((l) => l.closed), "a show scrolled away was let go");
  });
  scroll.scrollTop = 0;
  await wait(200);

  const offer = view.root.querySelector('button[data-act=send][data-row="ch:sunday-service/backup"][data-col="chd:sunday-service/youtube"]');
  offer.click();
  await until(() => dialogs().length && button(dialogs().at(-1), "Send it"));
  await wait(150);
  button(dialogs().at(-1), "Send it").click();
  await until(() => sent(stub, "channel.destination.set").length);
  test("an empty cell sends that stream there, in the format chosen", () => {
    const p = sent(stub, "channel.destination.set")[0].params;
    eq([p.id, p.destination, p.stream], ["sunday-service", "youtube", "backup"]);
  });
  closeAll();

  const filter = view.root.querySelector(".rt-filter");
  filter.value = "room 17";
  filter.dispatchEvent(new Event("input"));
  await wait(60);
  test("the filter narrows the grid to what matches", () => eq([...view.root.querySelectorAll(".rt-band strong")].map((s) => s.textContent), ["Room 17"]));

  toggleRouting(stub);
  await wait(60);
  test("closing lets go of every show and every event it asked for", () => {
    ok(!document.querySelector(".routing"), "gone");
    eq(linked(), []);
    ok(!stub.patterns.has("channel.*") && !stub.patterns.has("show.*"), [...stub.patterns.keys()].join(","));
  });

  const bare = showStub({ noShows: true });
  const lone = toggleRouting(bare);
  await until(() => lone.root.querySelector(".rt-band"));
  test("a core without shows is one show called This mixer", () => ok([...lone.root.querySelectorAll(".rt-band strong")].some((s) => s.textContent === "This mixer"), "found"));
  toggleRouting(bare);
}
