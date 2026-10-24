// The channel editor: add a channel, edit one, or start a new one from a
// copy. Its name (the public path), title and whether it runs; the input
// with an optional backup; the conversion, a profile or custom settings;
// the HLS output every channel has and any outputs beside it. Add sends
// headend.channel.add (Check sends it with dry_run first), Save sends
// headend.channel.update. What the station refuses is said in the dialog.
//
// A stream of the Flussonic file the station follows says, before it is
// saved, that saving takes it out of the file's hands. One taken over
// already offers Follow the file again.

import { el } from "../../shell/dom.js";
import { modal } from "../../shell/modal.js";
import { errorToast, toast } from "../../shell/toast.js";
import { sheet } from "./sheet.js";
import { NAME, duplicateOf, originOf, readSetup, takeOverNote } from "./setup.js";
import { choice, field, group, num, said, text, tick, whyBox } from "./form.js";
import { inputBox, outputsBox } from "./channel-parts.js";
import { convPicker } from "./conv.js";

const SEGMENTS = [["both", "MPEG-TS and CMAF"], ["ts", "MPEG-TS only"], ["cmaf", "CMAF only"]];
const ACCESS = [["open", "Open on the local network, no key"], ["keyed", "A viewer key in every link"]];

const BLANK = { name: "", enabled: true, input: { uri: "udp://@239.0.0.1:5000" }, profile: "copy", hls: { segments: "both", access: "open", segment_ms: 3400 }, outputs: [] };

/**
 * @param {object} client
 * @param {{entry?: object, copy?: boolean, setup?: object, onDone?: Function}} opts
 *   `entry` is a ChannelEntry to edit, or with `copy` to start a new one from.
 */
export async function editChannel(client, opts = {}) {
  sheet("headend");
  sheet("setup");
  const setup = opts.setup || (await readSetup(client));
  const entry = opts.entry || null;
  const editing = !!entry && !opts.copy;
  const names = new Set((setup.channels || []).map((e) => e.channel.name));
  const ch = entry ? (opts.copy ? duplicateOf(entry.channel, names) : structuredClone(entry.channel)) : structuredClone(BLANK);
  const hls = { segments: "both", access: "open", segment_ms: 3400, ...(ch.hls || {}) };

  const name = text(ch.name, "News", "Name", { maxlength: "64" });
  const title = text(ch.title || "", "News HD", "Title");
  const on = tick(ch.enabled !== false, "Running", "The channel runs");
  const main = inputBox(ch.input, "Input");
  const backup = inputBox((ch.input && ch.input.backup) || { uri: "" }, "Backup");
  const hasBackup = tick(!!(ch.input && ch.input.backup), "A backup input, taken when the main one stops");
  const conv = convPicker(setup, ch);
  const seg = choice(SEGMENTS, hls.segments, "Segments");
  const access = choice(ACCESS, hls.access, "Who may play it");
  const segLen = num(String(hls.segment_ms / 1000), "3.4", "Segment length, seconds", { min: "0.5", max: "20", step: "0.1" });
  const outs = outputsBox(ch.outputs);
  const why = whyBox();
  const path = el("code");
  const sayPath = () => { path.textContent = `/${name.value.trim() || "name"}/index.m3u8`; };
  name.addEventListener("input", sayPath);
  sayPath();

  const notes = [];
  if (editing && entry.follows) notes.push(el("p.hs-warn.hs-takeover", { text: takeOverNote(entry) }));
  if (editing && entry.detached_from) notes.push(el("div.hs-warn.hs-detached", {}, [el("span", { text: originOf(entry).full }), el("button.btn", { type: "button", text: "Follow the file again", onclick: () => follow() })]));
  if (editing && ch.origin) notes.push(el("p.hs-note.dim", { text: `Where it came from: ${ch.origin}.` }));

  const backupSlot = el("div.hs-backup", {}, [backup.node]);
  backupSlot.hidden = !hasBackup.box.checked;
  hasBackup.box.addEventListener("change", () => { backupSlot.hidden = !hasBackup.box.checked; });

  const body = el("div.hs-editor", {}, [
    ...notes,
    group("Channel", [
      el("div.hs-grid", {}, [field("Name", name, "Letters, digits, - _ and . only. It is the public path, so keep it once viewers have it."), field("Title", title, "What the screen calls it. Empty: the name.")]),
      el("p.hs-note.dim", {}, ["Players read it at ", path]), on.node,
    ]),
    group("Input", [main.node, hasBackup.node, backupSlot]),
    group("Conversion", [conv.node]),
    group("HLS output", [el("div.hs-grid", {}, [field("Segments", seg), field("Who may play it", access), field("Segment length, seconds", segLen, "Cut at a keyframe.")])]),
    group("More outputs", [el("p.hs-note.dim", { text: "Each one takes the channel's conversion unless Copy as it comes is ticked." }), outs.node]),
    why.node,
  ]);

  const cancel = el("button.btn", { type: "button", text: "Cancel" });
  const check = el("button.btn", { type: "button", text: "Check", hidden: editing, title: "Ask the station whether it would take this, and make nothing" });
  const save = el("button.btn.primary", { type: "button", text: editing ? "Save" : "Add the channel" });
  const heading = editing ? `Edit ${ch.name}` : entry ? `New channel from ${entry.channel.name}` : "Add a channel";
  const m = modal({ title: heading, wide: true, body, footer: [el("span.grow"), cancel, check, save] });
  m.el.classList.add("hs-dialog");
  cancel.onclick = () => m.close();

  const channel = () => {
    const c = {
      name: name.value.trim(),
      enabled: on.box.checked,
      input: main.read(),
      ...conv.value(),
      hls: { ...hls, segments: seg.value, access: access.value, segment_ms: Math.round((Number(segLen.value) || 3.4) * 1000) },
      outputs: outs.read(),
    };
    if (title.value.trim()) c.title = title.value.trim();
    if (hasBackup.box.checked && backup.uri()) c.input.backup = backup.read();
    return c;
  };
  const problem = (c) => {
    if (!NAME.test(c.name)) return "Give the channel a name of letters, digits, - _ and . only, up to 64.";
    if (!editing && names.has(c.name)) return `There is already a channel ${c.name}. Pick another name.`;
    if (!c.input.uri) return "Fill in the input's address.";
    return "";
  };

  async function send(dry) {
    const c = channel();
    const no = problem(c);
    if (no) return why.say(no);
    why.say([]);
    save.disabled = check.disabled = true;
    try {
      const got = editing ? await client.call("headend.channel.update", { name: entry.channel.name, channel: c }) : await client.call("headend.channel.add", { channels: [c], dry_run: dry });
      const refused = (got && got.refused) || [];
      if (refused.length) {
        why.say(refused.map((r) => r.why));
        why.node.scrollIntoView?.({ block: "nearest" });
      }
      else if (dry) toast({ text: `${c.name} would be added as it is.` });
      else {
        toast({ text: editing ? `${c.name} is saved.${(got.restarted || []).length ? " Its show was made again." : ""}` : `${c.name} is added and starting.` });
        m.close();
        if (opts.onDone) opts.onDone(got);
      }
    } catch (e) {
      why.say(said(e));
    }
    save.disabled = check.disabled = false;
  }

  async function follow() {
    try {
      const got = await client.call("headend.channel.follow", { names: [entry.channel.name] });
      if ((got.refused || []).length) return why.say(got.refused.map((r) => r.why));
      toast({ text: `${entry.channel.name} follows ${entry.detached_from} again.` });
      m.close();
      if (opts.onDone) opts.onDone(got);
    } catch (e) {
      errorToast(e, "The channel was not handed back to the file");
    }
  }

  save.onclick = () => send(false);
  check.onclick = () => send(true);
  return { dialog: m, channel, send };
}
