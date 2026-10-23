// The flow map against flowmap-stub.js: the words, the graph a station
// makes, where it lands, what a hover lights and a filter keeps, and the
// view itself: it asks for nothing until it opens and stops when it closes.
// No core needed.

import { flowStub } from "./flowmap-stub.js";
import { wait, until } from "./wall-help.js";

export async function flowmapTests(test, eq, ok) {
  const w = await import("../panels/flowmap/words.js");
  test("the flow map says rates, pictures and outputs in a broadcaster's words", () => {
    eq([w.rate(850), w.rate(3200), w.rate(48000), w.rate(1.2e6)], ["850 kbit/s", "3.2 Mbit/s", "48 Mbit/s", "1.2 Gbit/s"]);
    eq([w.picture(1280, 720, 25), w.picture(1920, 1080, 29.97, true), w.picture(1280, 720, 59.94)], ["720p", "1080i", "720p60"]);
    eq([{ platform: "youtube", uri_host: "rtmp://a.rtmp.youtube.com" }, { platform: "hls" }, { uri_host: "file:///rec" }, { uri_host: "udp://239.20.0.1:6000" }, { uri_host: "srt://h:9000" }].map(w.outputKind), ["YouTube", "HLS", "Recording", "Multicast", "SRT"]);
    eq(w.hlsFormat({ segments: "both", access: "open" }), "CMAF and TS, open");
    eq([w.sourceKind({ type: "camera", uri: "v4l2:///dev/video0" }), w.sourceKind({ type: "srt", uri: "srt://0.0.0.0:9000" }), w.sourceKind({ type: "browser", uri: "https://x" })], ["Camera", "SRT", "Browser"]);
  });

  const { build } = await import("../panels/flowmap/graph.js");
  const { layout } = await import("../panels/flowmap/layout.js");
  const { pathOf, kept } = await import("../panels/flowmap/path.js");
  const { Series } = await import("../panels/headend/data.js");
  const stub = flowStub({ n: 48 });
  const shows = (await stub.call("show.list")).shows;
  const stats = new Map((await stub.call("show.stats", {})).shows.map((s) => [s.id, s]));
  const series = new Map([...stats.keys()].map((id) => [id, new Series()]));
  const mixed = new Map([["main", { ...stub.state, plan: await stub.call("rendition.plan"), rates: new Map([["youtube", 6200]]) }]]);
  const gpu = await stub.call("gpu.status");
  const g = build({ shows, stats, series, mixed, gpu });
  const kinds = (k) => [...g.nodes.values()].filter((n) => n.kind === k);

  test("one input node per address however many programmes it feeds, and a node per show and output", () => {
    eq(kinds("input").length, 8, "48 channels six to a group");
    eq(g.nodes.get("in:239.10.0.1:5001").members.length, 6);
    eq(kinds("show").length, 49, "48 channels and the mixed show");
    eq(kinds("source").length, 6);
    ok(g.nodes.has("out:main:youtube") && g.nodes.has("out:main:multiview") && g.nodes.has("out:ch101:hls"), "outputs of both kinds of show");
  });

  test("a conversion is a node that says GPU and its steps; a copy is a straight link", () => {
    const p = g.nodes.get("proc:ch102:h264 1280x720 25 fps on nvh264enc");
    ok(p && p.gpu && p.steps.length === 3, "ch102 converts on the GPU");
    eq(p.title, "720p H.264");
    ok(g.links.get("show:ch101>out:ch101:hls").straight, "ch101 copies");
    ok(g.links.has(`gpu:0~${p.id}`), "the GPU is linked to what it converts");
  });

  test("a mixed show with two renditions draws each one, and its on air source is marked", () => {
    eq(kinds("proc").filter((n) => n.show === "main").map((n) => n.title).sort(), ["1080p H.264", "720p H.264"]);
    eq(g.nodes.get("src:main:cam1").tally, "program");
    ok(g.links.get("src:main:cam1>show:main").onair, "its link runs in the on air colour");
    eq(g.nodes.get("show:main").onAir, "Interview");
  });

  test("a dead group is a fault, a lossy one stutters, a stopped channel is off", () => {
    eq(g.nodes.get("in:239.10.0.4:5004").tone, "bad");
    const st = stats.get("ch107");
    series.get("ch107").take({ input: { ...st.input, cc_errors: 0 } }, 0);
    series.get("ch107").take({ input: { ...st.input, cc_errors: 9 } }, 1);
    const again = build({ shows, stats, series, mixed, gpu });
    ok(again.links.get("in>ch107").err > 0, "errors make the link stutter");
    eq(again.nodes.get("show:ch147").tone, "off");
  });

  const box = layout(g, false);
  test("the layout keeps a group's channels together under one input card, left to right by column", () => {
    const inp = g.nodes.get("in:239.10.0.1:5001");
    const members = inp.members.map((id) => g.nodes.get(`show:${id}`));
    ok(members.every((m) => m.y > inp.y - inp.h / 2 && m.y < inp.y + inp.h / 2), "each programme sits beside its input");
    ok(g.nodes.get("src:main:cam1").x < g.nodes.get("show:main").x && g.nodes.get("show:main").x < g.nodes.get("out:main:youtube").x, "columns run left to right");
    ok(box.width > 900 && box.height > 48 * 30, `${box.width} by ${box.height}`);
    const again = build({ shows, stats, series, mixed, gpu });
    layout(again, false);
    eq(again.nodes.get("show:ch140").y, g.nodes.get("show:ch140").y, "the same snapshot lands in the same place");
  });

  test("a phone folds the processing column into the links", () => {
    const small = build({ shows, stats, series, mixed, gpu });
    layout(small, true);
    ok(![...small.nodes.values()].some((n) => n.col === 2), "no processing nodes");
    ok(small.links.has("show:ch102>out:ch102:hls"), "the channel links straight to its output");
    eq(small.nodes.get("show:ch102").badge, "GPU");
  });

  test("hovering lights the whole path from input to outputs and nothing else", () => {
    const lit = pathOf(g, "show:ch102");
    ok(lit.has("in:239.10.0.1:5001") && lit.has("out:ch102:hls") && lit.has("gpu:0"), "its input, its output and the GPU it uses");
    ok(!lit.has("show:ch103"), "not the other channels on its input");
    const fromInput = pathOf(g, "in:239.10.0.1:5001");
    ok(["show:ch101", "show:ch106", "out:ch105:relay"].every((id) => fromInput.has(id)), "an input lights every programme it feeds");
  });

  test("a filter keeps the matching shows' paths: text, problems, input and kind of output", () => {
    ok(kept(g, {}) === null, "no filter, nothing dimmed");
    const srt = kept(g, { family: "srt" });
    ok(srt.has("out:ch108:srt") && !srt.has("show:ch101"), "only shows with an SRT output");
    ok(kept(g, { group: "239.10.0.2:5002" }).has("show:ch107") && !kept(g, { group: "239.10.0.2:5002" }).has("show:ch101"));
    ok(kept(g, { text: "ch115" }).has("in:239.10.0.3:5003"));
    const bad = kept(g, { problems: true });
    ok(bad.has("show:ch119") && !bad.has("show:ch101"), "a dead group's channel is a problem, a live copy is not");
  });

  await viewTests(test, eq, ok);
}

async function viewTests(test, eq, ok) {
  const stub = flowStub({ n: 48 });
  await wait(1200);
  eq(stub.calls.filter((c) => /^(show\.stats|gpu\.status|rendition\.plan)$/.test(c.method)).length, 0, "nothing is read before the map opens");
  const { toggleFlowMap } = await import("../panels/flowmap/view.js");
  const view = toggleFlowMap(stub);
  await until(() => view.map.graph && view.map.graph.nodes.size > 100, 4000);
  test("the map opens over the page with a canvas and draws the station", () => {
    ok(document.querySelector(".fm .fm-canvas"), "a canvas");
    ok(view.map.graph.nodes.size > 100, `${view.map.graph.nodes.size} nodes`);
    ok(stub.calls.some((c) => c.method === "gpu.status") && stub.calls.some((c) => c.method === "show.stats"));
  });
  view.side.open("ch102");
  await wait(100);
  test("a channel opens the headend's detail drawer beside the map", () => ok(!view.side.node.hidden && view.side.id === "ch102"));
  view.close();
  const after = stub.calls.length;
  await wait(2300);
  test("closing the map stops every read and takes it off the page", () => {
    ok(!document.querySelector(".fm"), "gone");
    eq(stub.calls.length, after, "no call after it closed");
  });
}
