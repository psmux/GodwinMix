// The headend's picture settings past size and bitrate, against the stubs:
// frame rate, source shape, deinterlace method and larger sources at their
// own size in a profile, the HLS ladder's rungs added and removed, a
// channel's own picture overrides in the editor and in a bulk change, and
// the words a conversion is described in. No station needed.

import { headendStub, someChannels } from "./headend-stub.js";
import { wait, until, sent, button, closeDialogs } from "./wall-help.js";

const by = (root, label) => root.querySelector(`[aria-label="${label}"]`);
const put = (root, label, value) => {
  const input = by(root, label);
  input.value = String(value);
  input.dispatchEvent(new Event("input", { bubbles: true }));
  input.dispatchEvent(new Event("change", { bubbles: true }));
};
const last = (stub, method) => sent(stub, method).at(-1);
const h264 = (width, height, kbps) => ({ codec: "h264", width, height, fit: "pad", bitrate_kbps: kbps, keyframe_ms: 2000, deinterlace: "auto" });

export async function headendPictureTests(test, eq, ok) {
  await modelTests(test, eq, ok);
  await profileTests(test, eq, ok);
  await editorTests(test, eq, ok);
  await bulkTests(test, eq, ok);
  closeDialogs();
}

async function modelTests(test, eq, ok) {
  const s = await import("../panels/headend/setup.js");
  const pic = await import("../panels/headend/picture.js");
  const { renditionWords } = await import("../panels/headend/import-report.js");
  const stubs = await import("./headend-stub.js");
  const want = {
    video: { ...h264(1280, 720, 1500), fps: { num: 25, den: 1 }, aspect: "4:3", deinterlace_method: "greedyh", match_source: { max_height: 1080, bitrate_kbps: 4500 } },
    ladder: [{ width: 854, height: 480, bitrate_kbps: 800 }],
    audio: { codec: "aac", bitrate_kbps: 128 },
  };
  test("frame rate, shape, method, match source and the ladder open and save as they came", () => eq(s.wantFrom(s.convFields(want)), want));
  test("the method goes only where a deinterlace runs, match source only with a size", () => {
    const off = s.wantFrom({ ...s.convFields(want), deinterlace: "off" }).video;
    ok(!("deinterlace" in off) && !("deinterlace_method" in off), JSON.stringify(off));
    ok(!("match_source" in s.wantFrom({ ...s.convFields(want), size: "source" }).video));
    eq(s.wantFrom({ ...s.convFields(want), codec: "copy" }).ladder, undefined, "no ladder without a picture");
  });
  test("a rung keeps what it came with, less match source", () => {
    eq(pic.rungFrom({ size: "640x360", bitrate_kbps: "500", fps: "", preset: "" }, { width: 854, height: 480, fit: "crop", match_source: { max_height: 1080 } }), { fit: "crop", width: 640, height: 360, bitrate_kbps: 500 });
    eq(pic.nextRung({ height: 720 }, []), { width: 1024, height: 576, bitrate_kbps: 1100 });
    ok(/same/.test(pic.ladderProblem({ height: 720 }, [{ height: 720 }])));
    eq(pic.ladderProblem({ height: 720, match_source: { max_height: 1080 } }, [{ height: 720 }]), "", "a 720p rung under 1080p of HD sources is no clash");
    eq(pic.nextRung({ height: 720, match_source: { max_height: 1080 } }, []).height, 720, "and it is the step offered first");
  });
  test("overrides: each choice left empty is the profile's, and off takes no method", () => {
    eq(pic.overridesFrom({ aspect: "", deinterlace: "", deinterlace_method: "", fit: "" }), {});
    eq(pic.overridesFrom({ aspect: "16:9", deinterlace: "off", deinterlace_method: "linear", fit: "crop" }), { aspect: "16:9", deinterlace: "off", fit: "crop" });
    eq(pic.overrideWords({ aspect: "4:3", deinterlace: "on", deinterlace_method: "greedyh" }), "shown as 4:3, deinterlace on, greedyh lines");
  });
  test("the built in ladder of HD sources is described as what it makes", () => {
    const { BUILTINS } = stubs;
    const p = BUILTINS.find((x) => x.id === "h264-hd-1080p-720p-ladder");
    ok(/larger sources at their own size up to 1080p at 4.5 Mb\/s · also 720p 1.5 Mb\/s for HLS/.test(s.profileWords(p)), s.profileWords(p));
  });
  test("a conversion's words say the shape, a method other than YADIF, match source and the ladder", () => {
    eq(renditionWords(want), "H.264 · 1280×720 · 25 fps · 1.5 Mb/s · pad #000000 · shown as 4:3 · deinterlace auto with greedyh · larger sources at their own size up to 1080p at 4.5 Mb/s · also 480p 800 kb/s for HLS · AAC 128 kb/s");
    ok(!/with yadif/.test(renditionWords({ video: { ...h264(1280, 720, 1500), deinterlace_method: "yadif" } })));
  });
}

async function profileTests(test, eq, ok) {
  const { manageProfiles } = await import("../panels/headend/profiles.js");
  const saved = [{ id: "steps", name: "Steps", video: h264(1920, 1080, 4000), ladder: [{ width: 1280, height: 720, bitrate_kbps: 1500, fit: "crop" }], audio: { codec: "aac", bitrate_kbps: 128 } }];
  const stub = headendStub({ channels: someChannels(), saved });
  const h = await manageProfiles(stub, {});
  const m = h.dialog.el;
  button(m, "New profile").click();
  put(m, "Profile name", "Lobby HD");
  test("the method is asked only while a deinterlace runs", () => {
    ok(!by(m, "Deinterlace method").closest(".field").hidden);
    put(m, "Deinterlace", "off");
    ok(by(m, "Deinterlace method").closest(".field").hidden);
    put(m, "Deinterlace", "auto");
  });
  put(m, "Source shape", "16:9");
  put(m, "Deinterlace method", "linear");
  put(m, "Frame rate", "25/1");
  put(m, "HD sources at their own size", "1080");
  put(m, "Bitrate at the larger size", "4500");
  button(m, "Add a size").click();
  button(m, "Add a size").click();
  const rungs = () => [...m.querySelectorAll(".hs-rung")];
  test("Add a size steps down from the tallest the main picture makes, then from the smallest rung", () => eq(rungs().map((r) => by(r, "Rung size").value), ["1280x720", "1024x576"]));
  by(rungs()[0], "Remove this size").click();
  put(rungs()[0], "Rung bitrate", "900");
  put(rungs()[0], "Rung frame rate", "25/1");
  test("a rung removed is gone from the form", () => eq(rungs().length, 1));
  button(m, "Save the profile").click();
  await until(() => sent(stub, "headend.profile.save").length);
  test("the profile is saved with its shape, method, match source and the one rung left", () => {
    const p = last(stub, "headend.profile.save").params.profile;
    eq([p.video.aspect, p.video.deinterlace_method, p.video.fps, p.video.match_source], ["16:9", "linear", { num: 25, den: 1 }, { max_height: 1080, bitrate_kbps: 4500 }]);
    eq(p.ladder, [{ width: 1024, height: 576, bitrate_kbps: 900, fps: { num: 25, den: 1 } }]);
  });
  await wait(30);
  button(m.querySelector('.hs-pcard[data-id="steps"]'), "Edit").click();
  test("a saved ladder opens with its rungs, and Advanced closed when nothing in it is used", () => {
    eq(rungs().map((r) => [by(r, "Rung size").value, by(r, "Rung bitrate").value]), [["1280x720", "1500"]]);
    ok(!m.querySelector(".hs-ppane .hs-more").open);
  });
  button(m, "Add a size").click();
  put(rungs()[1], "Rung size", "1280x720");
  button(m, "Save").click();
  await wait(20);
  test("two rungs of one size are refused before anything is sent", () => {
    ok(/same/.test(m.querySelector(".hs-ppane .hs-why").textContent));
    eq(sent(stub, "headend.profile.save").length, 1);
  });
  by(rungs()[1], "Remove this size").click();
  button(m, "Save").click();
  await until(() => sent(stub, "headend.profile.save").length > 1);
  test("saved untouched, a rung keeps what the form does not show", () => eq(last(stub, "headend.profile.save").params.profile.ladder, [{ fit: "crop", width: 1280, height: 720, bitrate_kbps: 1500 }]));
  h.dialog.close();
}

async function editorTests(test, eq, ok) {
  const { editChannel } = await import("../panels/headend/editor.js");
  const stub = headendStub({ channels: someChannels() });
  const news = () => stub.setup.channels.find((e) => e.channel.name === "News");
  let h = await editChannel(stub, { entry: news() });
  let m = h.dialog.el;
  test("a channel with no overrides opens on the profile's for each", () => eq(["Source shape", "Deinterlace", "Deinterlace method", "Another shape"].map((l) => by(m, l).value), ["", "", "", ""]));
  put(m, "Source shape", "4:3");
  put(m, "Deinterlace", "on");
  put(m, "Deinterlace method", "greedyh");
  put(m, "Another shape", "crop");
  button(m, "Save").click();
  await until(() => sent(stub, "headend.channel.update").length);
  test("the editor sends the channel's own picture settings as video_overrides", () => eq(last(stub, "headend.channel.update").params.channel.video_overrides, { aspect: "4:3", deinterlace: "on", deinterlace_method: "greedyh", fit: "crop" }));
  await wait(20);
  h = await editChannel(stub, { entry: news() });
  m = h.dialog.el;
  test("and opens on them again", () => eq(by(m, "Source shape").value, "4:3"));
  for (const l of ["Source shape", "Deinterlace", "Deinterlace method", "Another shape"]) put(m, l, "");
  button(m, "Save").click();
  await until(() => sent(stub, "headend.channel.update").length > 1);
  test("every one back at the profile's sends none", () => ok(!("video_overrides" in last(stub, "headend.channel.update").params.channel)));
  closeDialogs();
}

async function bulkTests(test, eq, ok) {
  const { channelList } = await import("../panels/headend/channels.js");
  const stub = headendStub({ channels: someChannels() });
  const h = await channelList(stub, {});
  const m = h.dialog.el;
  by(m, "Tick News").checked = true;
  by(m, "Tick News").dispatchEvent(new Event("change", { bubbles: true }));
  by(m, "Tick Sport").checked = true;
  by(m, "Tick Sport").dispatchEvent(new Event("change", { bubbles: true }));
  m.querySelector(".hs-bulkpic").open = true;
  put(m, "Ticked channels' source shape", "16:9");
  put(m, "Ticked channels' deinterlace", "auto");
  button(m, "Set picture overrides").click();
  await until(() => sent(stub, "headend.channel.bulk").length);
  await wait(30);
  test("Set picture overrides sends them for every ticked channel", () => eq(last(stub, "headend.channel.bulk").params, { names: ["News", "Sport"], video_overrides: { aspect: "16:9", deinterlace: "auto" } }));
  test("and the list says which channels have their own", () => ok(/Own picture: shown as 16:9, deinterlace auto/.test(m.querySelector('tr[data-name="News"]').textContent)));
  button(m, "Clear picture overrides").click();
  await until(() => sent(stub, "headend.channel.bulk").length > 1);
  await wait(30);
  test("Clear sends an empty set, so each takes its profile as it is", () => {
    eq(last(stub, "headend.channel.bulk").params, { names: ["News", "Sport"], video_overrides: {} });
    ok(!stub.setup.channels.some((e) => e.channel.video_overrides));
    ok(!/Own picture/.test(m.querySelector('tr[data-name="News"]').textContent));
  });
  h.dialog.close();
}
