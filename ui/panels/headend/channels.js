// Every channel the station knows, in a table: its name, input, profile,
// state and where it came from, with a tick box each for the changes
// made to many at once (a profile, on or off, the network card,
// removal), and Edit and Duplicate on each row. Opened from the band of
// the headend view; reads headend.channel.list each time it changes.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { toast } from "../../shell/toast.js";
import { sheet } from "./sheet.js";
import { inputWords, originOf, profileOf, readSetup } from "./setup.js";
import { choice, said, text, whyBox } from "./form.js";

const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

/**
 * @param {object} client
 * @param {{stateOf?: (showId: string) => string, onChange?: Function}} opts
 */
export async function channelList(client, opts = {}) {
  sheet("headend");
  sheet("setup");
  let setup = await readSetup(client);
  const picked = new Set();
  const refused = new Map();
  let armed = 0;
  let filter = "";

  const find = text("", "Find a channel, an address or a profile", "Find a channel", { type: "search" });
  const all = el("input", { type: "checkbox", "aria-label": "Tick every channel shown" });
  const tbody = el("tbody");
  const table = el("table.he-table.hs-table", {}, [
    el("thead", {}, [el("tr", {}, [el("th", {}, [all]), ...["Channel", "Input", "Profile", "State", "From", ""].map((h) => el("th", { text: h }))])]),
    tbody,
  ]);
  const empty = el("div.hs-none", { hidden: true });
  const count = el("span.hs-count");
  const profile = el("select", { "aria-label": "Profile for the ticked channels" });
  const nic = text("", "eth1", "Network card for the ticked channels", { size: "8" });
  const why = whyBox();
  const removeB = el("button.btn.he-danger", { type: "button", "data-bulk": "remove" });
  const bulk = el("div.hs-bulk", { hidden: true }, [
    count,
    el("span.hs-row", {}, [profile, el("button.btn", { type: "button", "data-bulk": "profile", text: "Set profile" })]),
    el("button.btn", { type: "button", "data-bulk": "on", text: "Enable" }),
    el("button.btn", { type: "button", "data-bulk": "off", text: "Disable" }),
    el("span.hs-row", {}, [nic, el("button.btn", { type: "button", "data-bulk": "nic", text: "Move to card" })]),
    removeB,
  ]);
  const top = el("div.hs-row.hs-listtop", {}, [
    find,
    el("span.grow"),
    el("button.btn.primary", { type: "button", "data-go": "add", text: "Add a channel" }),
    el("button.btn", { type: "button", "data-go": "scan", text: "Scan a feed" }),
    el("button.btn", { type: "button", "data-go": "profiles", text: "Profiles" }),
    el("button.btn", { type: "button", "data-go": "import", text: "Import" }),
  ]);
  const body = el("div.hs-list", {}, [top, bulk, why.node, el("div.hs-scroll", {}, [table]), empty]);
  const m = modal({ title: "Channels", wide: true, body, footer: [el("span.grow"), el("button.btn", { type: "button", text: "Close", onclick: () => m.close() })] });
  m.el.classList.add("hs-dialog", "hs-wide");

  const shown = () => setup.channels.filter((e) => {
    if (!filter) return true;
    const c = e.channel;
    const hay = [c.name, c.title, c.input && c.input.uri, profileOf(setup, c).name, originOf(e).word].join(" ").toLowerCase();
    return filter.split(/\s+/).every((w) => hay.includes(w));
  });

  const stateWord = (e) => {
    if (e.channel.enabled === false) return ["off", "Off"];
    const w = e.show && opts.stateOf ? opts.stateOf(e.show) : "";
    return w ? [w.toLowerCase().replace(/\s+/g, ""), w] : ["on", "On"];
  };

  function draw() {
    const rows = shown();
    const names = new Set(setup.channels.map((e) => e.channel.name));
    for (const n of [...picked]) if (!names.has(n)) picked.delete(n);
    tbody.replaceChildren(...rows.flatMap((e) => {
      const c = e.channel;
      const o = originOf(e);
      const [sk, sw] = stateWord(e);
      const box = el("input", { type: "checkbox", "data-pick": c.name, "aria-label": `Tick ${c.name}` });
      box.checked = picked.has(c.name);
      const tr = el(`tr.hs-r${picked.has(c.name) ? ".picked" : ""}`, { "data-name": c.name }, [
        el("td", {}, [box]),
        el("td", { "data-label": "Channel" }, [el("strong", { text: c.name }), c.title ? el("div.he-sub", { text: c.title }) : null]),
        el("td.hs-wrap", { "data-label": "Input", text: inputWords(c.input) }),
        el("td", { "data-label": "Profile", text: profileOf(setup, c).name || c.profile }),
        el("td", { "data-label": "State" }, [el(`span.hs-state.st-${sk}`, { text: sw })]),
        el("td", { "data-label": "From", title: o.full }, [el(`span.hs-from.from-${o.key}`, { text: o.word })]),
        el("td.hs-acts", {}, [
          el("button.btn", { type: "button", "data-row": "edit", text: "Edit" }),
          el("button.btn", { type: "button", "data-row": "copy", text: "Duplicate" }),
          e.detached_from ? el("button.btn", { type: "button", "data-row": "follow", text: "Follow the file again" }) : null,
        ]),
      ]);
      const no = refused.get(c.name);
      return no ? [tr, el("tr.hs-whyrow", {}, [el("td", { colspan: "7", text: no })])] : [tr];
    }));
    all.checked = rows.length > 0 && rows.every((e) => picked.has(e.channel.name));
    empty.hidden = setup.channels.length > 0 && rows.length > 0;
    table.hidden = !rows.length;
    empty.replaceChildren(setup.missing
      ? el("p", { text: `This station keeps no headend channels: ${said(setup.error)}` })
      : !setup.channels.length ? el("p", { text: "No channels yet. Add one, scan a multicast feed for its programs, or import a Flussonic config." })
      : el("p", { text: "No channel matches that." }));
    const n = picked.size;
    bulk.hidden = !n;
    count.textContent = `${plural(n, "channel", "channels")} ticked`;
    const want = setup.profiles.map((p) => p.id).join("|");
    if (profile.dataset.ids !== want) {
      const v = profile.value;
      profile.dataset.ids = want;
      profile.replaceChildren(...setup.profiles.map((p) => el("option", { value: p.id, text: p.name || p.id })));
      if (v) profile.value = v;
    }
    const followed = [...picked].filter((x) => (setup.channels.find((e) => e.channel.name === x) || {}).follows).length;
    removeB.textContent = armed > Date.now() ? `Press again to remove ${plural(n, "channel", "channels")}` : "Remove";
    removeB.title = followed ? `${plural(followed, "of them is a stream", "of them are streams")} of the followed file: removing hides it there, and the file is not changed.` : "Remove the channels and their shows";
  }

  async function reload() {
    setup = await readSetup(client);
    draw();
    if (opts.onChange) opts.onChange(setup);
  }

  /** One bulk call; refusals are said under their rows. */
  async function run(method, params, done) {
    refused.clear();
    why.say([]);
    try {
      const got = await client.call(method, params);
      for (const r of got.refused || []) refused.set(r.name, r.why);
      if ((got.done || []).length) toast({ text: done(got.done.length) });
      for (const n of got.done || []) if (method === "headend.channel.remove") picked.delete(n);
    } catch (e) {
      why.say(said(e));
    }
    await reload();
  }

  async function bulkAct(what) {
    const names = [...picked];
    if (!names.length) return;
    if (what === "profile") return run("headend.channel.bulk", { names, profile: profile.value }, (n) => `${plural(n, "channel takes", "channels take")} ${profile.selectedOptions[0].textContent} now.`);
    if (what === "on" || what === "off") return run("headend.channel.bulk", { names, enabled: what === "on" }, (n) => `${plural(n, "channel is", "channels are")} ${what === "on" ? "on" : "off"}.`);
    if (what === "nic") return run("headend.channel.bulk", { names, interface: nic.value.trim() }, (n) => `${plural(n, "channel joins", "channels join")} on ${nic.value.trim() || "the default route's card"}.`);
    if (what === "remove") {
      if (armed < Date.now()) {
        armed = Date.now() + 4000;
        setTimeout(draw, 4100);
        return draw();
      }
      armed = 0;
      return run("headend.channel.remove", { names }, (n) => `${plural(n, "channel was", "channels were")} removed.`);
    }
  }

  async function rowAct(what, name) {
    const entry = setup.channels.find((e) => e.channel.name === name);
    if (!entry) return;
    if (what === "follow") return run("headend.channel.follow", { names: [name] }, () => `${name} follows the file again.`);
    const { editChannel } = await import("./editor.js");
    editChannel(client, { entry, copy: what === "copy", setup, onDone: reload });
  }

  async function go(what) {
    if (what === "add") return (await import("./editor.js")).editChannel(client, { setup, onDone: reload });
    if (what === "scan") return (await import("./scan.js")).scanFeed(client, { setup, onDone: reload });
    if (what === "profiles") return (await import("./profiles.js")).manageProfiles(client, { setup, onChange: reload });
    if (what === "import") return (await import("./import.js")).importFlussonic(client, { onDone: reload });
  }

  find.addEventListener("input", () => { filter = find.value.trim().toLowerCase(); draw(); });
  all.addEventListener("change", () => {
    for (const e of shown()) all.checked ? picked.add(e.channel.name) : picked.delete(e.channel.name);
    armed = 0;
    draw();
  });
  body.addEventListener("change", (e) => {
    const box = e.target.closest("[data-pick]");
    if (!box) return;
    box.checked ? picked.add(box.dataset.pick) : picked.delete(box.dataset.pick);
    armed = 0;
    draw();
  });
  body.addEventListener("click", (e) => {
    const b = e.target.closest("[data-bulk], [data-row], [data-go]");
    if (!b) return;
    if (b.dataset.bulk) return bulkAct(b.dataset.bulk);
    if (b.dataset.go) return go(b.dataset.go);
    rowAct(b.dataset.row, b.closest("tr").dataset.name);
  });

  draw();
  return { dialog: m, reload, get setup() { return setup; }, picked };
}
