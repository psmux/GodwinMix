// Stopping a recording from wherever the person is: the header's REC button
// with its clock, the question it asks, and Outputs > Stop recording when
// nothing is recording. Against a stand in client, so no file is written.

import { get as command } from "../shell/commands.js";
import { stopRecording, recordings } from "../panels/outputs/record-stop.js";

const tick = (ms = 0) => new Promise((r) => setTimeout(r, ms));

async function until(predicate, what, ms = 3000) {
  const started = Date.now();
  while (!predicate()) {
    if (Date.now() - started > ms) throw new Error(`timed out waiting for ${what}`);
    await tick(20);
  }
}

const RECORDING = {
  id: "recording-m1x2", type: "record/output", state: "live", uri_host: "record://programme/…",
  has_key: true, reconnects: 0, queue_secs: 0, bytes_muxed: 1048576, recording_secs: 65,
  recording_path: "C:\\Users\\ana\\Videos\\GodwinMix\\recording-m1x2-0.mp4",
};

function standIn(outputs) {
  const renders = new Set();
  const client = {
    sent: [],
    state: { connected: true, outputs, uptime_secs: 10 },
    call: async (method, params) => {
      client.sent.push([method, params]);
      if (method === "output.remove") {
        client.state = { ...client.state, outputs: client.state.outputs.filter((o) => o.id !== params.id) };
        renders.forEach((fn) => fn(client.state));
      }
      return {};
    },
    on: () => () => {},
    onRender: (fn) => (renders.add(fn), () => renders.delete(fn)),
  };
  return client;
}

const dialog = () => [...document.querySelectorAll(".dialog")].find((d) => d.getAttribute("aria-label") === "Stop recording");

export async function recordingStopTests(test, eq, ok) {
  // A dialog an earlier suite left open holds Escape for itself.
  window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
  await tick();

  test("only record outputs count as recordings", () => {
    eq(recordings({ outputs: [RECORDING, { id: "yt", type: "rtmp/output", state: "live" }] }).map((o) => o.id), ["recording-m1x2"]);
    eq(recordings({}), []);
  });

  window.godwinmixPanels ||= [];
  await import("../panels/header/panel.js");
  const client = standIn([RECORDING]);
  const header = document.createElement("gmx-header");
  header.setClient(client);
  document.body.append(header);
  const rec = header.querySelector(".hdr-rec");

  test("REC is a button on the header with the recording's running time", () => {
    ok(rec, "the header has a REC control");
    eq(rec.tagName, "BUTTON");
    ok(!rec.hidden, "REC is shown while a recording runs");
    ok(/^REC 1:0[5-9]$/.test(rec.textContent), `REC shows the time: ${rec.textContent}`);
    ok(/stop/i.test(rec.getAttribute("aria-label")), "it says what pressing it does");
  });

  rec.click();
  await until(() => dialog(), "the Stop recording question");
  test("pressing REC asks before stopping, and names the file", () => {
    ok(dialog().textContent.includes("recording-m1x2-0.mp4"), "the file is named");
    eq(client.sent.filter(([m]) => m === "output.remove").length, 0);
  });
  [...dialog().querySelectorAll("button")].find((b) => b.textContent === "Keep recording").click();
  await tick();
  test("Keep recording leaves it running", () => {
    ok(!dialog(), "the question closed");
    eq(client.sent.filter(([m]) => m === "output.remove").length, 0);
  });

  rec.click();
  await until(() => dialog(), "the question again");
  [...dialog().querySelectorAll("button")].find((b) => b.textContent === "Stop recording").click();
  await until(() => !dialog(), "the question to close after stopping");
  test("Stop recording removes it through output.remove and REC goes away", () => {
    eq(client.sent.filter(([m]) => m === "output.remove"), [["output.remove", { id: "recording-m1x2" }]]);
    ok(rec.hidden, "REC is hidden with nothing recording");
  });
  header.remove();

  const starting = standIn([{ ...RECORDING, state: "connecting", recording_secs: 0, bytes_muxed: 0 }]);
  const second = document.createElement("gmx-header");
  second.setClient(starting);
  document.body.append(second);
  test("a recording that has not started writing can still be stopped from REC", () => {
    const button = second.querySelector(".hdr-rec");
    ok(!button.hidden, "REC is shown for a recording that is starting");
    ok(!button.classList.contains("live"), "it is not red until a file is growing");
  });
  second.remove();

  const idle = standIn([]);
  const before = document.querySelectorAll(".toast").length;
  const none = stopRecording(idle);
  test("Stop recording with nothing recording says so and calls nothing", () => {
    eq(none, null);
    eq(idle.sent, []);
    ok(document.querySelectorAll(".toast").length > before, "a toast says nothing is recording");
  });

  test("Outputs > Stop recording is a registered command", () => ok(command("output.stop-recording"), "registered"));
}
