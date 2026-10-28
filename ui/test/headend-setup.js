// A headend set up from its own screen, against the stubs: the three ways
// in on an empty station, the channel editor's add and edit (an address
// built from the fields and read back), the note before a followed
// stream is taken over, a scan whose ticked programs are added in one
// call, a bulk change on the channel list, and a profile whose removal is
// refused until its channels move. No station needed.

import { headendStub, someChannels } from "./headend-stub.js";
import { wait, until, sent, button, closeDialogs, dialogs } from "./wall-help.js";

const by = (root, label) => root.querySelector(`[aria-label="${label}"]`);
const put = (root, label, value) => {
  const input = by(root, label);
  if (input.type === "checkbox") input.checked = !!value;
  else input.value = String(value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
};
const last = (stub, method) => sent(stub, method).at(-1);

export async function headendSetupTests(test, eq, ok) {
  await modelTests(test, eq, ok);
  await firstRunTests(test, eq, ok);
  await editorTests(test, eq, ok);
  await scanTests(test, eq, ok);
  await listTests(test, eq, ok);
  await profileTests(test, eq, ok);
  closeDialogs();
}

async function modelTests(test, eq, ok) {
  const s = await import("../panels/headend/setup.js");
  test("an input's address is built from its fields and read back into them", () => {
    eq(s.inputUri({ kind: "udp-m", group: "239.0.0.1", port: "5000" }), "udp://@239.0.0.1:5000");
    eq(s.inputUri({ kind: "udp-u", port: "5000" }), "udp://0.0.0.0:5000");
    eq(s.inputUri({ kind: "rtp-m", group: "239.0.0.3", port: "5004" }), "rtp://@239.0.0.3:5004");
    eq(s.inputUri({ kind: "srt", address: ":9000" }), "srt://@:9000");
    eq(s.inputUri({ kind: "rtmp", url: "192.0.2.20/live/key" }), "rtmp://192.0.2.20/live/key");
    eq(s.inputUri({ kind: "file", path: "/clips/loop.ts" }), "file:///clips/loop.ts");
    const f = s.inputFields({ uri: "udp://@239.0.0.1:5000", program: 7, interface: "eth1" });
    eq([f.kind, f.group, f.port, f.program, f.interface], ["udp-m", "239.0.0.1", "5000", "7", "eth1"]);
    eq(s.inputFields({ uri: "udp://0.0.0.0:6000" }).kind, "udp-u");
    eq(s.inputFields({ uri: "rtp://@239.0.0.3:5004" }).kind, "rtp-m");
    eq(s.inputFields({ uri: "srt://@:9000" }).address, ":9000");
    eq(s.inputFields({ uri: "https://example.com/live/index.m3u8" }).kind, "hls");
    eq(s.inputFields({ uri: "file:///clips/loop.ts" }).path, "/clips/loop.ts");
  });
  test("custom settings make a profile's wants and open on them again", () => {
    const want = { video: { codec: "h264", width: 1280, height: 720, fit: "crop", bitrate_kbps: 2500, keyframe_ms: 2000, deinterlace: "auto", encoder: "gpu" }, audio: { codec: "aac", bitrate_kbps: 96, tracks: "all" } };
    eq(s.wantFrom(s.convFields(want)), want);
    eq(s.wantFrom(s.convFields({})), {}, "a pass through stays one");
  });
  test("a scanned program is named from the pattern, else P and its number", () => {
    eq(s.scanName("{name}", { number: 101, name: "News HD" }), "News-HD");
    eq(s.scanName("{name}", { number: 102, name: "" }), "P102");
    eq(s.scanName("CH{number}", { number: 7, name: "x" }), "CH7");
  });
}

async function firstRunTests(test, eq, ok) {
  const { openHeadend } = await import("../panels/headend/view.js");
  const stub = headendStub();
  const view = openHeadend(stub);
  await until(() => !view.root.querySelector(".he-first").hidden);
  test("an empty headend offers three ways in, and import is only one of them", () => {
    const ways = [...view.root.querySelectorAll(".he-first [data-way]")].map((b) => b.querySelector("strong").textContent);
    eq(ways, ["Add a channel", "Scan a multicast feed", "Import a Flussonic config"]);
    for (const act of ["add", "scan", "channels", "import"]) ok(view.root.querySelector(`.he-band [data-act="${act}"]`), `the band has ${act}`);
  });
  view.root.querySelector('.he-first [data-way="add"]').click();
  await until(() => dialogs().some((d) => d.getAttribute("aria-label") === "Add a channel"));
  test("Add a channel opens the editor", () => ok(dialogs().at(-1).querySelector(".hs-editor")));
  closeDialogs();
  view.root.querySelector('.he-band [data-act="scan"]').click();
  await until(() => dialogs().some((d) => d.getAttribute("aria-label") === "Scan a multicast feed"));
  test("the band's scan opens the scan", () => ok(dialogs().at(-1).querySelector(".hs-scan")));
  closeDialogs();
  view.close();

  const kept = headendStub({ channels: someChannels() });
  const v2 = openHeadend(kept);
  await until(() => v2.setup && v2.root.querySelector(".he-tile"));
  v2.pick("news");
  await wait(40);
  test("a channel the station keeps has Edit channel in its drawer", () => {
    ok(v2.root.querySelector(".he-first").hidden, "no first run with channels");
    ok(button(v2.side.node, "Edit channel"));
  });
  button(v2.side.node, "Edit channel").click();
  await until(() => dialogs().some((d) => d.getAttribute("aria-label") === "Edit News"));
  test("and it opens the editor on that channel", () => eq(by(dialogs().at(-1), "Input group").value, "239.0.0.1"));
  closeDialogs();
  v2.close();
}

async function editorTests(test, eq, ok) {
  const { editChannel } = await import("../panels/headend/editor.js");
  const stub = headendStub({ channels: someChannels() });
  let h = await editChannel(stub, {});
  let m = h.dialog.el;
  put(m, "Name", "CH200");
  put(m, "Input transport", "udp-m");
  put(m, "Input group", "239.0.0.5");
  put(m, "Input port", "5000");
  put(m, "Input network card", "eth1");
  put(m, "Input program number", "7");
  put(m, "Profile", "h264-720p-1500");
  test("the editor shows the address its fields make", () => eq(m.querySelector(".hs-uri").textContent, "udp://@239.0.0.5:5000"));
  button(m, "Add the channel").click();
  await until(() => sent(stub, "headend.channel.add").length);
  test("Add sends a multicast input with its program and card", () => eq(last(stub, "headend.channel.add").params, {
    channels: [{ name: "CH200", enabled: true, input: { uri: "udp://@239.0.0.5:5000", program: 7, interface: "eth1" }, profile: "h264-720p-1500", hls: { segments: "both", access: "open", segment_ms: 3400 }, outputs: [] }],
    dry_run: false,
  }));
  await wait(20);
  test("and closes once the station has made it", () => ok(!m.isConnected));

  h = await editChannel(stub, {});
  m = h.dialog.el;
  put(m, "Name", "Relay");
  put(m, "Input transport", "srt");
  put(m, "Input address", "192.0.2.10:9000");
  put(m, "Input passphrase", "secret1234");
  put(m, "Input latency, ms", "200");
  put(m, "Profile", "custom");
  await wait(10);
  test("custom settings offer the GPU on a machine that has one", () => ok([...by(m, "Encoder").options].some((o) => o.value === "gpu")));
  put(m, "Video bitrate", "2500");
  put(m, "Encoder", "gpu");
  put(m, "Sound tracks", "all");
  button(m, "Add the channel").click();
  await until(() => sent(stub, "headend.channel.add").length > 1);
  test("an SRT input and custom settings on the GPU go out as the station takes them", () => {
    const c = last(stub, "headend.channel.add").params.channels[0];
    eq(c.input, { uri: "srt://192.0.2.10:9000", params: { passphrase: "secret1234", latency_ms: 200 } });
    eq(c.profile, "custom");
    eq(c.custom, { video: { codec: "h264", width: 1280, height: 720, fit: "pad", bitrate_kbps: 2500, keyframe_ms: 2000, deinterlace: "auto", encoder: "gpu" }, audio: { codec: "aac", bitrate_kbps: 128, tracks: "all" } });
  });

  h = await editChannel(stub, {});
  m = h.dialog.el;
  put(m, "Name", "News");
  put(m, "Input group", "239.0.0.8");
  put(m, "Input port", "5000");
  button(m, "Add the channel").click();
  await wait(30);
  test("a name the station has is refused in the dialog before it is sent", () => {
    ok(/already a channel News/.test(m.querySelector(".hs-why").textContent));
    eq(sent(stub, "headend.channel.add").length, 2);
  });
  put(m, "Name", "bad name!");
  button(m, "Check").click();
  await wait(20);
  test("so is a name that is not a path", () => ok(/letters, digits/.test(m.querySelector(".hs-why").textContent)));
  h.dialog.close();

  const nopu = headendStub({ channels: someChannels(), gpu: false });
  h = await editChannel(nopu, {});
  put(h.dialog.el, "Profile", "custom");
  await wait(10);
  test("and no GPU where the machine has none", () => ok(![...by(h.dialog.el, "Encoder").options].some((o) => o.value === "gpu")));
  h.dialog.close();

  const entries = () => stub.setup.channels;
  h = await editChannel(stub, { entry: entries().find((e) => e.channel.name === "News") });
  m = h.dialog.el;
  test("an edit opens on the channel's own input, profile and title", () => {
    eq([by(m, "Input transport").value, by(m, "Input group").value, by(m, "Input port").value, by(m, "Input network card").value, by(m, "Input program number").value], ["udp-m", "239.0.0.1", "5000", "eth1", "101"]);
    eq(by(m, "Profile").value, "h264-720p-1500");
    eq(by(m, "Title").value, "News HD");
    ok(!m.querySelector(".hs-takeover"), "a channel kept here has no take over note");
  });
  h.dialog.close();

  h = await editChannel(stub, { entry: entries().find((e) => e.channel.name === "Sport") });
  m = h.dialog.el;
  test("a followed stream says before saving that saving takes it out of the file's hands", () => {
    const note = m.querySelector(".hs-takeover");
    ok(note && /takes it out of the file's hands/.test(note.textContent) && /later edits to this stream in the file are ignored/.test(note.textContent), note && note.textContent);
    eq([by(m, "Input transport").value, by(m, "Input address").value, by(m, "Input latency, ms").value], ["srt", "192.0.2.10:9000", "200"]);
    ok(m.querySelector(".hs-out input[type=checkbox]").checked, "its UDP out copies as it comes");
  });
  button(m, "Save").click();
  await until(() => sent(stub, "headend.channel.update").length);
  test("saved untouched, it goes back as it came", () => eq(last(stub, "headend.channel.update").params, {
    name: "Sport",
    channel: { name: "Sport", enabled: true, input: { uri: "srt://192.0.2.10:9000", params: { latency_ms: 200 } }, profile: "copy", hls: { segments: "both", access: "open", segment_ms: 3400 }, outputs: [{ id: "push-1", uri: "udp://239.0.1.1:5000", copy: true }] },
  }));

  h = await editChannel(stub, { entry: entries().find((e) => e.channel.name === "CH101") });
  m = h.dialog.el;
  test("a channel taken over offers to follow the file again", () => ok(button(m, "Follow the file again")));
  button(m, "Follow the file again").click();
  await until(() => sent(stub, "headend.channel.follow").length);
  test("which hands it back with headend.channel.follow", () => eq(last(stub, "headend.channel.follow").params, { names: ["CH101"] }));
  closeDialogs();
}

async function scanTests(test, eq, ok) {
  const { scanFeed } = await import("../panels/headend/scan.js");
  const stub = headendStub({ channels: someChannels(), scanMs: 250 });
  let h = await scanFeed(stub, {});
  let m = h.dialog.el;
  put(m, "Group", "239.0.0.9");
  put(m, "Port", "5000");
  put(m, "Network card", "eth1");
  button(m, "Scan").click();
  await wait(30);
  test("a scan shows that it is listening, and for how long", () => {
    ok(!m.querySelector(".hs-progress").hidden);
    ok(/Listening to udp:\/\/@239.0.0.9:5000 on eth1 for up to 8 s/.test(m.querySelector(".hs-saying").textContent));
  });
  await until(() => m.querySelector(".hs-table tbody tr"));
  test("then lists every program with its picture, sound and what is already a channel", () => {
    eq(last(stub, "headend.scan").params, { uri: "udp://@239.0.0.9:5000", wait_ms: 8000, interface: "eth1" });
    eq(m.querySelectorAll(".hs-table tbody tr[data-prog]").length, 4);
    const text = m.querySelector(".hs-table").textContent;
    ok(/H264 1920×1080i 25 fps/.test(text) && /MP2 stereo fra/.test(text), text);
    ok(/Scrambled/.test(text) && /Already channel Sport/.test(text));
  });
  for (const n of [101, 102]) put(m, `Tick program ${n}`, true);
  put(m, "Profile for every one", "h264-480p-800");
  test("each ticked program shows the name it becomes", () => eq([...m.querySelectorAll("tr[data-prog] td:last-child")].map((td) => td.textContent).slice(0, 2), ["News-HD", "P102"]));
  button(m, "Add 2 channels").click();
  await until(() => sent(stub, "headend.channel.add").length);
  test("two ticked programs are added with one profile in one call", () => {
    eq(sent(stub, "headend.channel.add").length, 1);
    const p = last(stub, "headend.channel.add").params;
    eq(p.dry_run, false);
    eq(p.channels.map((c) => [c.name, c.title, c.input, c.profile]), [
      ["News-HD", "News HD", { uri: "udp://@239.0.0.9:5000", program: 101, interface: "eth1" }, "h264-480p-800"],
      ["P102", undefined, { uri: "udp://@239.0.0.9:5000", program: 102, interface: "eth1" }, "h264-480p-800"],
    ]);
  });
  await wait(20);
  test("and the scan closes when every one was made", () => ok(!m.isConnected));

  h = await scanFeed(stub, {});
  m = h.dialog.el;
  put(m, "Group", "239.0.0.9");
  put(m, "Port", "5000");
  button(m, "Scan").click();
  await until(() => m.querySelector(".hs-table tbody tr"));
  put(m, "Tick program 104", true);
  button(m, "Add 1 channel").click();
  await until(() => m.querySelector(".hs-whyrow"));
  test("a program the station refuses says why in its own row", () => ok(/already a channel Sport/.test(m.querySelector(".hs-whyrow").textContent)));
  h.dialog.close();
}

async function listTests(test, eq, ok) {
  const { channelList } = await import("../panels/headend/channels.js");
  const stub = headendStub({ channels: someChannels() });
  const h = await channelList(stub, {});
  const m = h.dialog.el;
  const row = (name) => m.querySelector(`tr[data-name="${name}"]`);
  test("the list says where each channel came from", () => {
    eq(m.querySelectorAll("tbody tr[data-name]").length, 3);
    eq(["News", "Sport", "CH101"].map((n) => row(n).querySelector(".hs-from").textContent), ["Added here", "Followed file", "Taken from the file"]);
    ok(/239.0.0.1:5000, program 101 on eth1/.test(row("News").textContent));
    ok(m.querySelector(".hs-bulk").hidden, "no bulk bar until something is ticked");
  });
  put(m, "Tick News", true);
  put(m, "Tick CH101", true);
  put(m, "Profile for the ticked channels", "h264-480p-800");
  button(m, "Set profile").click();
  await until(() => sent(stub, "headend.channel.bulk").length);
  test("Set profile sends one bulk change for every ticked channel", () => eq(last(stub, "headend.channel.bulk").params, { names: ["News", "CH101"], profile: "h264-480p-800" }));
  await wait(30);
  button(m, "Disable").click();
  await until(() => sent(stub, "headend.channel.bulk").length > 1);
  test("Disable sends enabled false", () => eq(last(stub, "headend.channel.bulk").params, { names: ["News", "CH101"], enabled: false }));
  await wait(30);
  button(m, "Remove").click();
  await wait(20);
  test("Remove asks for a second press first", () => {
    eq(sent(stub, "headend.channel.remove").length, 0);
    ok(button(m, "Press again to remove 2 channels"));
  });
  button(m, "Press again to remove 2 channels").click();
  await until(() => sent(stub, "headend.channel.remove").length);
  await wait(30);
  test("then removes them", () => {
    eq(last(stub, "headend.channel.remove").params, { names: ["News", "CH101"] });
    eq(m.querySelectorAll("tbody tr[data-name]").length, 1);
  });
  h.dialog.close();
}

async function profileTests(test, eq, ok) {
  const { manageProfiles } = await import("../panels/headend/profiles.js");
  const stub = headendStub({ channels: someChannels() });
  const h = await manageProfiles(stub, {});
  const m = h.dialog.el;
  const card = (id) => m.querySelector(`.hs-pcard[data-id="${id}"]`);
  test("built in profiles are read only, saved ones can be changed", () => {
    ok(!button(card("h264-720p-1500"), "Edit") && !button(card("h264-720p-1500"), "Remove") && button(card("h264-720p-1500"), "Duplicate"));
    ok(button(card("lobby"), "Edit") && button(card("lobby"), "Remove"));
  });
  button(card("lobby"), "Remove").click();
  await until(() => card("lobby").querySelector(".hs-move"));
  test("a removal refused because a channel takes the profile asks where it moves", () => {
    eq(last(stub, "headend.profile.remove").params, { id: "lobby" });
    ok(/CH101 takes lobby/.test(card("lobby").textContent));
  });
  put(card("lobby"), "Move the channels to", "h264-720p-1500");
  button(card("lobby"), "Move them and remove").click();
  await until(() => !card("lobby"));
  test("and removes it with move_to", () => {
    eq(last(stub, "headend.profile.remove").params, { id: "lobby", move_to: "h264-720p-1500" });
    eq(stub.setup.channels.find((e) => e.channel.name === "CH101").channel.profile, "h264-720p-1500");
  });
  button(m, "New profile").click();
  put(m, "Profile name", "Lobby 720p");
  put(m, "Encoder", "cpu");
  button(m, "Save the profile").click();
  await until(() => sent(stub, "headend.profile.save").length);
  test("a new profile is saved with its id made from its name", () => {
    const p = last(stub, "headend.profile.save").params.profile;
    eq([p.id, p.name, p.video.encoder, p.video.width], ["lobby-720p", "Lobby 720p", "cpu", 1280]);
  });
  h.dialog.close();
}
