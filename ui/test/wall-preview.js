// The monitoring wall, bulk add and a show's detail against wall-stub.js,
// one state per address, for looking at and for screenshots:
// /test/wall.html?scene=rows&theme=dark
//
// Scenes: rows, tiles, grouped, bulk, bulk-dry, detail, empty. Nothing here
// talks to a core.

import { wallStub } from "./wall-stub.js";

const q = new URLSearchParams(location.search);
const scene = q.get("scene") || "rows";
const theme = q.get("theme") || "dark";
document.getElementById("gmx-theme").href = `../themes/${theme}.css`;
document.documentElement.dataset.theme = theme;
const wait = (ms = 80) => new Promise((r) => setTimeout(r, ms));
const until = async (fn, ms = 3000) => { const end = Date.now() + ms; while (!fn() && Date.now() < end) await wait(30); return fn(); };

function bar() {
  const h = document.querySelector(".slot-header");
  Object.assign(h.style, { height: "44px", display: "flex", alignItems: "center", padding: "0 16px", fontWeight: "600" });
  h.textContent = "GodwinMix";
}

async function main() {
  bar();
  try { localStorage.removeItem("gmx.wall"); } catch { /* fine */ }
  const stub = wallStub({ n: scene === "empty" ? 0 : Number(q.get("n") || 200) });
  const { toggleWall } = await import("../panels/wall/view.js");
  const view = toggleWall(stub);
  await until(() => view.root.querySelector(".wl-row, .wl-tile, .wl-empty:not([hidden])"));
  if (scene === "tiles") view.root.querySelector('[data-top="tiles"]').click();
  if (scene === "grouped") view.root.querySelector('[data-top="group"]').click();
  await wait(1300);
  if (q.get("cursor")) { view.cursor = q.get("cursor"); view.draw(); }
  if (scene.startsWith("bulk")) await bulk(stub);
  if (scene === "detail") {
    const { showDetail } = await import("../panels/wall/detail.js");
    showDetail(stub, stub.shows[Number(q.get("i") || 1)], { data: view.data });
    await wait(700);
  }
  document.body.dataset.ready = "1";
}

async function bulk(stub) {
  const { bulkAdd } = await import("../panels/wall/bulk.js");
  const b = bulkAdd(stub);
  if (scene === "bulk") return wait(200);
  b.paste([
    "name,input,program,output,format",
    "BBC One,udp://@239.10.0.1:5000,1,udp://10.0.0.60:6000,copy",
    "BBC Two,udp://@239.10.0.2:5000,1,udp://10.0.0.60:6002,copy",
    "ITV,udp://@239.10.0.3:5000,2,srt://cdn.example:7000,youtube-720p30",
    "Channel 4,udp://@239.10.0.4:5000,,srt://cdn.example:7002,youtube-720p30",
    "News 1,udp://@239.10.0.5:5000,,udp://10.0.0.60:6008,copy",
    "Film4,mms://239.10.0.6:5000,,udp://10.0.0.60:6010,copy",
  ].join("\n"));
  await b.check();
  await wait(300);
}

main();
