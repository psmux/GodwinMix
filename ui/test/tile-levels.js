// A tile's level and the face of a source with no picture.
//
// The report this answers: a USB microphone added through the audio-device
// plugin was live, the core was measuring it, and nothing on screen moved. Its
// tile showed a black picture, and the only level it had sat in the strip that
// appears on hover, fourteen pixels wide. These tests hold the level out where
// a person sees it and the black box away from a source with nothing to show.

import { buildTile, syncTile, setTileMode } from "../panels/sources/tile.js";
import { soundOnly, watchLevel } from "../panels/sources/tile-level.js";
import { meterClient, dropViews, takeMeters } from "../shell/meter.js";
import { Client } from "../client/index.js";
import { Store } from "../client/store.js";

const settle = (ms) => new Promise((r) => setTimeout(r, ms));
const deps = (meters) => ({ audio: { bindFader() {} }, scrub: {}, onMute: () => {}, meters });
const MIC = { id: "mic", uri: "audio-device/source", type: "audio-device/source", has_video: false, has_audio: true, state: "live", gain: 1 };
const CAM = { id: "cam", uri: "test://smpte", has_video: true, has_audio: true, state: "live", gain: 1 };

export async function tileLevelTests(test, eq, ok) {
  test("a microphone's tile says sound only and draws no picture", () => {
    const tile = buildTile(MIC, deps(true));
    ok(tile.sound, "a source with sound and no video is not sound only");
    ok(tile.node.querySelector(".soundface"), "no face where the black picture was");
    eq(tile.note.textContent, "Sound only, no video");
    for (const mode of ["live", "snapshot", "icon"]) {
      setTileMode(tile, mode);
      ok(tile.pic.hidden && tile.still.hidden && tile.kindbox.hidden, `a picture box is showing in ${mode} mode`);
    }
  });

  test("the level sits outside the hover strip, so it is on screen without a pointer", () => {
    for (const source of [MIC, CAM]) {
      const tile = buildTile(source, deps(true));
      ok(tile.meter, `${source.id} has no level`);
      ok(!tile.strip.contains(tile.meter), `${source.id}'s level is still in the hover strip`);
      ok(tile.meter.classList.contains("h") && tile.meter.classList.contains("level"), "not the horizontal tile level");
      ok(tile.node.classList.contains("metered"), "the strip will not move clear of the level");
    }
  });

  test("a microphone that has sent nothing yet still says what it is", () => {
    const quiet = { ...MIC, has_audio: false, state: "connecting" };
    ok(soundOnly(quiet, "mic"), "a microphone with no samples yet lost its face");
    const tile = buildTile(quiet, deps(true));
    eq(tile.note.textContent, "Sound only, nothing heard yet");
    ok(tile.meter, "a microphone with no samples yet has no level to show when they come");
    syncTile(tile, MIC, {});
    eq(tile.note.textContent, "Sound only, no video", "the words did not follow the sound arriving");
  });

  test("a camera still connecting keeps its picture box", () => {
    const connecting = { ...CAM, has_video: false, has_audio: false, state: "connecting" };
    ok(!soundOnly(connecting, "camera"), "every source would flash sound only while it connects");
    ok(!buildTile(connecting, deps(true)).sound);
  });

  test("Show meters on tiles off means no level and nothing asked of the core", () => {
    const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
    meterClient(client);
    const tile = buildTile(MIC, deps(false));
    eq(tile.meter, null);
    watchLevel(tile);
    eq(client.extSpec(), {}, "levels were asked for with meters turned off");
    ok(tile.node.querySelector(".soundface"), "the face went with the meter");
    meterClient(null);
  });

  // The whole path the page takes: the tile is watched, the ask goes to the
  // core, an `event/meters` batch arrives and the bar is drawn lit.
  const client = new Client({ name: "test", subscribe: () => Promise.resolve({}) }, new Store());
  meterClient(client);
  const tile = buildTile(MIC, deps(true));
  document.body.appendChild(tile.node);
  watchLevel(tile);
  const asked = JSON.stringify(client.extSpec());
  for (let i = 0; i < 6; i += 1) {
    takeMeters({ program: [], sources: { mic: [-6, -6] } });
    await settle(50);
  }
  const fills = [...tile.meter.querySelectorAll(".fill")].map((f) => f.style.clipPath);
  const readout = tile.readout.textContent;
  dropViews("tile:mic");
  const released = JSON.stringify(client.extSpec());
  tile.node.remove();
  meterClient(null);
  test("a level from the core lights the microphone's bar and prints its peak", () => {
    eq(asked, JSON.stringify({ meters: true }), "a tile level on screen did not ask for levels");
    eq(fills.length, 2, "a stereo input should draw two bars");
    // At -6 dBFS the bar is about three quarters lit: the right inset is small.
    for (const f of fills) {
      const right = Number(/inset\(0(?:px)? ([\d.]+)%/.exec(f)?.[1]);
      ok(right > 20 && right < 32, `the bar is not lit to -6 dB: ${f}`);
    }
    eq(readout, "-6.0");
    eq(released, "{}", "the ask outlived the tile");
  });
}
