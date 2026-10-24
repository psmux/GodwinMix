// Scan a multicast feed: the station listens to a group and port for a
// few seconds (headend.scan) and lists each program it carries with its
// name, provider, picture and sound. Tick the ones wanted, pick one
// profile for them all and how they are named, and they are added in one
// headend.channel.add. A program the station refuses says why in its row.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { sheet } from "./sheet.js";
import { inputUri, readSetup, scanName, trackWords } from "./setup.js";
import { choice, field, num, said, text, whyBox } from "./form.js";

const WAIT_MS = 8000;
const KINDS = [["udp-m", "UDP multicast"], ["rtp-m", "RTP multicast"], ["udp-u", "UDP unicast to this machine"]];

/** The params of headend.scan from the fields, or {error}. */
export function scanParams(f) {
  const uri = String(f.raw || "").trim() || inputUri(f);
  if (!uri) return { error: f.kind === "udp-u" ? "Give the port the feed is sent to." : "Give the group and the port of the feed." };
  const out = { uri, wait_ms: Number(f.wait_ms) || WAIT_MS };
  if (String(f.interface || "").trim()) out.interface = String(f.interface).trim();
  return out;
}

/** The channels the ticked programs become. */
export function scannedChannels(scan, progs, o) {
  return progs.map((p) => {
    const c = { name: scanName(o.pattern, p), enabled: true, input: { uri: scan.uri, program: p.number }, profile: o.profile, hls: { segments: "both", access: "open", segment_ms: 3400 } };
    if (scan.interface) c.input.interface = scan.interface;
    if (p.name) c.title = p.name;
    return c;
  });
}

export async function scanFeed(client, opts = {}) {
  sheet("headend");
  sheet("setup");
  const setup = opts.setup || (await readSetup(client));
  const kind = choice(KINDS, "udp-m", "Transport");
  const groupI = text("", "239.0.0.1", "Group");
  const port = num("", "5000", "Port", { min: "1", max: "65535" });
  const nic = text("", "eth1", "Network card");
  const raw = text("", "udp://@239.0.0.1:5000", "Address as typed");
  const wait = choice([["4000", "4 seconds"], ["8000", "8 seconds"], ["15000", "15 seconds"]], String(WAIT_MS), "Listen for");
  const ask = el("div.hs-scanask", {}, [
    el("p.hs-note.dim", { text: "The station listens to the feed for a few seconds and lists every program in it. Nothing is added until you pick." }),
    el("div.hs-grid", {}, [field("Transport", kind), field("Group", groupI, "A multicast group, 239.0.0.1 say."), field("Port", port), field("Network card", nic, "Empty: the default route's.")]),
    el("details.hs-more", {}, [el("summary", { text: "Advanced" }), el("div.hs-grid", {}, [field("Address as typed", raw, "Taken over the fields above when set."), field("Listen for", wait)])]),
  ]);
  const progress = el("div.hs-progress", { hidden: true, role: "status" }, [el("span.hs-saying"), el("div.hs-bar", {}, [el("span")])]);
  const out = el("div.hs-scanout", { hidden: true });
  const why = whyBox();
  const scanB = el("button.btn.primary", { type: "button", text: "Scan" });
  const again = el("button.btn", { type: "button", text: "Scan again", hidden: true });
  const addB = el("button.btn.primary", { type: "button", hidden: true, disabled: true, text: "Add the ticked programs" });
  const close = el("button.btn", { type: "button", text: "Cancel" });
  const m = modal({ title: "Scan a multicast feed", wide: true, body: el("div.hs-scan", {}, [ask, progress, why.node, out]), footer: [el("span.grow"), close, again, scanB, addB] });
  m.el.classList.add("hs-dialog", "hs-wide");
  close.onclick = () => m.close();
  const sync = () => { groupI.closest(".field").hidden = kind.value === "udp-u"; };
  kind.addEventListener("change", sync);
  sync();

  let scan = null;
  const ticked = new Set();
  const rowWhy = new Map();
  const profile = choice((setup.profiles || []).map((p) => [p.id, p.name || p.id]), "copy", "Profile for every one");
  const pattern = text("{name}", "{name}", "Naming pattern");
  const preview = el("span.hs-note.dim");

  function table() {
    const progs = scan.programs || [];
    const all = el("input", { type: "checkbox", "aria-label": "Tick every program" });
    all.checked = progs.length > 0 && progs.every((p) => ticked.has(p.number));
    all.onchange = () => { for (const p of progs) all.checked ? ticked.add(p.number) : ticked.delete(p.number); draw(); };
    const rows = progs.flatMap((p) => {
      const t = trackWords(p);
      const box = el("input", { type: "checkbox", "data-prog": String(p.number), "aria-label": `Tick program ${p.number}` });
      box.checked = ticked.has(p.number);
      const notes = [p.scrambled ? el("span.hs-flag.bad", { text: "Scrambled" }) : null, p.channel ? el("span.hs-flag", { text: `Already channel ${p.channel}` }) : null, p.kbps ? el("span.dim", { text: `${(p.kbps / 1000).toFixed(1)} Mb/s` }) : null];
      const tr = el(`tr.hs-r${ticked.has(p.number) ? ".picked" : ""}`, { "data-prog": String(p.number) }, [
        el("td", {}, [box]),
        el("td.num", { "data-label": "Program", text: String(p.number) }),
        el("td", { "data-label": "Name" }, [el("strong", { text: p.name || "no name sent" }), p.provider ? el("div.he-sub", { text: p.provider }) : null]),
        el("td.hs-wrap", { "data-label": "Picture", text: t.video }),
        el("td.hs-wrap", { "data-label": "Sound", text: t.sound }),
        el("td", { "data-label": "Notes" }, notes),
        el("td.dim", { "data-label": "Becomes", text: ticked.has(p.number) ? scanName(pattern.value, p) : "" }),
      ]);
      const no = rowWhy.get(p.number);
      return no ? [tr, el("tr.hs-whyrow", {}, [el("td", { colspan: "7", text: no })])] : [tr];
    });
    return el("div.hs-scroll", {}, [el("table.he-table.hs-table", {}, [
      el("thead", {}, [el("tr", {}, [el("th", {}, [all]), ...["#", "Name", "Picture", "Sound", "", "Becomes"].map((h) => el("th", { text: h }))])]),
      el("tbody", {}, rows),
    ])]);
  }

  function draw() {
    const progs = scan.programs || [];
    const head = el("p.hs-scansum", { text: progs.length ? `${progs.length} ${progs.length === 1 ? "program" : "programs"} on ${scan.uri}, listened for ${(scan.listened_ms / 1000).toFixed(1)} s.` : `Nothing was read from ${scan.uri} in ${(scan.listened_ms / 1000).toFixed(1)} s.` });
    const n = ticked.size;
    addB.disabled = !n;
    addB.textContent = n ? `Add ${n} ${n === 1 ? "channel" : "channels"}` : "Add the ticked programs";
    preview.textContent = "{name} is the name the feed sends, {number} the program number. A program with no name becomes P and its number.";
    out.replaceChildren(head, scan.error ? el("p.hs-warn", { text: scan.error }) : null, progs.length ? table() : null, progs.length ? el("div.hs-grid.hs-scanpick", {}, [field("Profile for every one", profile), field("Naming pattern", pattern), preview]) : null);
  }

  out.addEventListener("change", (e) => {
    const box = e.target.closest("input[data-prog]");
    if (!box) return;
    const n = Number(box.dataset.prog);
    box.checked ? ticked.add(n) : ticked.delete(n);
    draw();
  });
  pattern.addEventListener("input", () => {
    for (const td of out.querySelectorAll("tr[data-prog] td:last-child")) {
      const p = scan.programs.find((x) => x.number === Number(td.parentNode.dataset.prog));
      td.textContent = ticked.has(p.number) ? scanName(pattern.value, p) : "";
    }
  });

  async function listen() {
    const params = scanParams({ kind: kind.value, group: groupI.value, port: port.value, interface: nic.value, raw: raw.value, wait_ms: wait.value });
    if (params.error) return why.say(params.error);
    why.say([]);
    scanB.disabled = again.disabled = true;
    ask.hidden = true;
    out.hidden = true;
    progress.hidden = false;
    progress.querySelector(".hs-saying").textContent = `Listening to ${params.uri}${params.interface ? ` on ${params.interface}` : ""} for up to ${params.wait_ms / 1000} s`;
    const bar = progress.querySelector(".hs-bar span");
    bar.style.animationDuration = `${params.wait_ms}ms`;
    bar.classList.remove("go");
    void bar.offsetWidth;
    bar.classList.add("go");
    try {
      scan = { ...(await client.call("headend.scan", params)), interface: params.interface };
      ticked.clear();
      rowWhy.clear();
      out.hidden = false;
      addB.hidden = false;
      again.hidden = false;
      scanB.hidden = true;
      draw();
    } catch (e) {
      ask.hidden = false;
      why.say(said(e));
    }
    progress.hidden = true;
    scanB.disabled = again.disabled = false;
  }

  async function add() {
    const progs = scan.programs.filter((p) => ticked.has(p.number));
    if (!progs.length) return;
    const channels = scannedChannels(scan, progs, { pattern: pattern.value, profile: profile.value });
    addB.disabled = true;
    try {
      const got = await client.call("headend.channel.add", { channels, dry_run: false });
      rowWhy.clear();
      for (const r of got.refused || []) if (progs[r.index]) rowWhy.set(progs[r.index].number, r.why);
      const done = (got.done || []).length;
      if (done) toast({ text: `${done} ${done === 1 ? "channel was" : "channels were"} added.${rowWhy.size ? ` ${rowWhy.size} refused, the reason is in each row.` : ""}` });
      for (const p of progs) if (!rowWhy.has(p.number)) ticked.delete(p.number);
      if (opts.onDone && done) opts.onDone(got);
      if (!rowWhy.size) return m.close();
      draw();
    } catch (e) {
      why.say(said(e));
      addB.disabled = false;
    }
  }

  scanB.onclick = listen;
  again.onclick = () => { ask.hidden = false; out.hidden = true; addB.hidden = true; again.hidden = true; scanB.hidden = false; };
  addB.onclick = add;
  return { dialog: m, listen, add, fields: { kind, group: groupI, port, nic, raw }, profile, pattern };
}
