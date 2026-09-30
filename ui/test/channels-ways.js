// Channels on every protocol: the addresses an encoder is given for RTMP,
// SRT and WHIP, the line that says which ports are open and why, and the
// Ways in switches in a channel's settings. Against the stub, no core.

const wait = (ms = 30) => new Promise((r) => setTimeout(r, ms));
const top = () => [...document.querySelectorAll(".dialog")].at(-1);
const closeAll = () => { for (const d of document.querySelectorAll(".dialog")) d.parentElement.remove(); };

export async function waysTests(test, eq, ok, stub, card, view) {
  const ways = await import("../panels/channels/ways.js");
  const { settingsParams } = await import("../panels/channels/edit.js");
  const { editChannel } = await import("../panels/channels/edit.js");
  const c = { app: "sun", key_mode: "query", protocols: ["rtmp", "srt", "whip"], rtmps: { enabled: true, port: 443 } };

  test("the ways into a channel are listed in the order the page shows them", () => {
    eq(ways.waysIn(c), ["rtmp", "rtmps", "srt", "whip"]);
    eq(ways.waysIn({ app: "old" }), ["rtmp"], "a channel from before protocols is RTMP");
  });

  test("SRT is a server, a stream id and the key as the passphrase", () => {
    const f = ways.waysFields("srt", c, "KEY", "srt://192.168.1.20:9000", "cam2");
    eq(f.rows.map((r) => r[0]), ["Server", "Stream ID", "Passphrase", "Full URL"]);
    eq(f.url, "srt://192.168.1.20:9000?streamid=sun/cam2&passphrase=KEY");
    const byName = ways.waysFields("srt", { ...c, key_mode: "stream" }, "KEY", "srt://h:9000");
    eq(byName.url, "srt://h:9000?streamid=sun/KEY", "the key is the stream name, and no passphrase");
  });

  test("WHIP is one URL on the page's port and the key as the bearer token", () => {
    const f = ways.waysFields("whip", c, "KEY", "http://10.0.0.5:8080");
    eq(f.rows, [["WHIP URL", "http://10.0.0.5:8080/whip/sun/main", false], ["Bearer token", "KEY", true]]);
  });

  test("the ports line names each open port, its protocol and the channels it is open for", () => {
    const rows = [
      { protocol: "rtmp", transport: "tcp", port: 19381, open: true, because: ["sun"] },
      { protocol: "srt", transport: "udp", port: 19382, open: true, because: ["sun", "hall"] },
      { protocol: "whip", transport: "tcp", port: 18681, open: false, because: [] },
    ];
    eq(ways.openPorts(rows), "Open ports: RTMP 19381 for sun · SRT 19382/udp for sun, hall");
    eq(ways.openPorts([]), "No port is open for encoders.");
    const stuck = [{ protocol: "srt", port: 9000, open: false, because: ["sun"], problem: "another program holds it" }];
    eq(ways.portProblems(stuck), ["SRT 9000 is not open: another program holds it"]);
  });

  test("saving the ways in sends only what changed", () => {
    const was = { id: "sun", name: "Sun", enabled: true, auto_source: true, key_mode: "query", protocols: ["rtmp"], rtmps: { enabled: false, port: 443 } };
    const same = { name: "Sun", protocols: ["rtmp"], rtmps: { enabled: false, port: 8443 } };
    eq(settingsParams(was, same), { id: "sun" }, "a port typed while RTMPS is off changes nothing");
    eq(settingsParams(was, { ...same, protocols: ["srt", "rtmp"] }), { id: "sun", protocols: ["srt", "rtmp"] });
    eq(settingsParams(was, { ...same, rtmps: { enabled: true, port: 443 } }), { id: "sun", rtmps: { enabled: true, port: 443 } });
  });

  // SRT and WHIP on, the way a person does it: the settings, two switches, Save.
  editChannel(view, view.model.byId.get("sunday-service"));
  const labels = [...top().querySelectorAll(".chn-waysin .chn-toggle")];
  for (const name of ["SRT", "WHIP"]) {
    const box = labels.find((l) => l.textContent.includes(name)).querySelector("input");
    box.checked = true;
  }
  [...top().querySelectorAll("button")].find((b) => b.textContent.trim() === "Save").click();
  await wait(60);
  test("Save sends the protocols switched on", () => {
    const set = stub.calls.filter((x) => x.method === "channel.set").at(-1);
    eq(set.params, { id: "sunday-service", protocols: ["rtmp", "srt", "whip"] });
  });
  closeAll();
  await view.load();

  test("the panel's line says which ports are open, for which channel", () => {
    const line = document.querySelector(".chn-ports").textContent;
    ok(line.includes("SRT 9000/udp for sunday-service"), line);
    ok(line.includes("WHIP 8080 (this page's port)"), line);
  });

  const box = () => card().querySelector(".chn-connectbox");
  // The section keeps the address and the stream name chosen in the tests
  // before this one: 192.168.1.20 and main_720p.
  const values = () => [...box().querySelectorAll(".chn-ckey .chn-obsval code")].map((x) => x.textContent);
  card().querySelector(".chn-connect").click();
  await wait();
  const picker = box().querySelector(".chn-ways");
  test("Connect offers a choice of protocol once there is more than one", () => {
    ok(!picker.hidden, "the protocol picker shows");
    eq([...picker.querySelectorAll("button")].map((b) => b.textContent), ["RTMP", "SRT", "WHIP"]);
  });
  [...picker.querySelectorAll("button")].find((b) => b.textContent === "SRT").click();
  await wait();
  test("SRT shows its own server, stream id and passphrase, the key hidden", () => {
    eq(values()[0], "srt://192.168.1.20:9000");
    eq(values()[1], "sunday-service/main_720p");
    ok(values()[2].startsWith("•"), values()[2]);
  });
  [...box().querySelectorAll("button")].find((b) => b.textContent.includes("Show")).click();
  await wait(60);
  test("Show puts the key in as the passphrase and in the full URL", () => {
    const secret = stub.secrets.get("key-1");
    eq(values()[2], secret);
    eq(values()[3], `srt://192.168.1.20:9000?streamid=sunday-service/main_720p&passphrase=${secret}`);
  });
  [...picker.querySelectorAll("button")].find((b) => b.textContent === "WHIP").click();
  await wait();
  test("WHIP shows the URL on the page's port and the bearer token", () => {
    eq(values()[0], "http://192.168.1.20:8080/whip/sunday-service/main_720p");
    eq(values()[1], stub.secrets.get("key-1"));
  });
  card().querySelector(".chn-connect").click();
  await wait();
}
