// Show tabs and the routing view against the stub, one state per address,
// for looking at and for screenshots: /test/shows.html?scene=three
//
// Scenes: one, three, menu, new, routing, routing-filter, big (twenty
// inputs and twenty outputs). Nothing here talks to a core.

import { showStub, sundayChannels } from "./shows-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "three";
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;

const wait = (ms = 80) => new Promise((r) => setTimeout(r, ms));

async function header(stub) {
  window.godwinmixPanels ||= [];
  await import("../panels/header/panel.js");
  const h = document.createElement("gmx-header");
  h.setClient(stub);
  document.querySelector(".slot-header").append(h);
  await wait(150);
  return h;
}

async function main() {
  const one = scene === "one";
  const stub = showStub(one ? { shows: [{ id: "main", name: "Sunday service", state: "running", on_air: "Worship" }] } : scene === "big" ? await big() : {});
  stub.state = { ...stub.state, connected: true, scene: "Worship", outputs: [{ id: "youtube", state: "live" }, { id: "facebook", state: "live" }] };
  await header(stub);
  if (scene === "menu") document.querySelector('.showtab[aria-selected="true"]').click();
  if (scene === "new") (await import("../shell/show-file.js")).newShow(stub);
  if (scene === "big") await bigChannels(stub);
  if (scene.startsWith("routing")) await sundayChannels(stub);
  if (scene.startsWith("routing") || scene === "big") {
    const view = (await import("../panels/routing/view.js")).toggleRouting(stub);
    if (scene === "routing-filter") {
      await wait(200);
      const f = view.root.querySelector(".rt-filter");
      f.value = q.get("filter") || "youtube";
      f.dispatchEvent(new Event("input"));
    }
  }
  await wait(500);
  document.body.dataset.ready = "1";
}

/** Five shows, each with a programme, two sources and two outputs. */
async function big() {
  const { sundayShows, sundayDetail } = await import("./shows-stub.js");
  const shows = [...sundayShows().slice(0, 2), ...["Chapel", "Gym", "Foyer"].map((name) => ({ id: name.toLowerCase(), name, state: "running", on_air: "Camera" }))];
  const detail = sundayDetail();
  for (const s of shows.slice(2)) {
    detail[s.id] = {
      sources: [{ id: `${s.id}-cam`, name: `${s.name} camera` }, { id: `${s.id}-slides`, name: "Slides" }],
      tally: { [`${s.id}-cam`]: "program" },
      outputs: [{ id: `${s.id}-stream`, type: "rtmp/output", uri_host: "rtmp://a.rtmp.youtube.com/", state: "live" }, { id: `${s.id}-screen`, type: "srt/output", uri_host: "srt://10.0.0.50:9000", state: "live", rendition: { preset: "youtube-720p30" } }],
      plan: { nodes: [{ kind: "encode", serves: [`${s.id}-screen`], encoder: "x264", cost: { cpu_millicores: 900 } }] },
    };
  }
  return { shows, detail };
}

/** Five channels, two streams and two destinations each. */
async function bigChannels(stub) {
  const { liveStream } = await import("./channels-stub.js");
  for (const name of ["North campus", "South campus", "Youth", "Radio", "Overflow"]) {
    const id = (await stub.call("channel.add", { name })).channel.id;
    stub.channels.get(id).streams = [liveStream("main", { protocol: "srt" }), liveStream("backup", { protocol: "rtmp", since_ms: Date.now() - 1000 })];
    await stub.call("channel.destination.add", { id, platform: "youtube", server: "rtmp://a.rtmp.youtube.com/live2", key: "k" });
    await stub.call("channel.destination.add", { id, platform: "facebook", server: "rtmps://live-api-s.facebook.com:443/rtmp", key: "k", rendition: { preset: "facebook-720p30" } });
    stub.plans[`channel:${id}`] = { nodes: [{ kind: "copy", serves: ["youtube"] }, { kind: "encode", serves: ["facebook"], encoder: { id: "h264-videotoolbox", hardware: true }, cost: { cpu_millicores: 70 } }] };
  }
}

main();
