// The wall's Channels group: one item per channel stream from channel.list,
// how many destinations are sending, red while one fails, what a filter
// keeps, and a row and a tile drawn from an item. No core needed.

export const CHANNELS = {
  channels: [
    {
      id: "church", name: "Church", enabled: true,
      streams: [
        { name: "main", state: "live", protocol: "rtmp", video: { codec: "h264", width: 1920, height: 1080, fps: 30, kbps: 4500 }, audio: { codec: "aac", channels: 2, sample_rate: 48000, kbps: 128 }, source: "church-main" },
        { name: "backup", state: "idle", video: null, audio: null },
      ],
      destinations: [
        { id: "youtube", label: "YouTube", stream: "*", enabled: true, state: "live" },
        { id: "facebook", label: "Facebook", stream: "main", enabled: true, state: "reconnecting", error: "the far end closed the connection" },
        { id: "twitch", label: "Twitch", stream: "main", enabled: true, state: "live" },
        { id: "kick", label: "Kick", stream: "main", enabled: false, state: "off" },
        { id: "vimeo", label: "Vimeo", stream: "backup", enabled: true, state: "waiting" },
      ],
    },
    { id: "annex", name: "Annex", enabled: true, streams: [], destinations: [] },
    { id: "old", name: "Old", enabled: false, streams: [], destinations: [] },
  ],
};

export async function wallChannelTests(test, eq, ok) {
  const m = await import("../panels/wall/channel-model.js");
  const items = m.channelItems(CHANNELS);
  test("the wall lists every channel stream, and a channel nothing publishes to once", () => {
    eq(items.map((it) => it.key), ["annex", "church/backup", "church/main", "old"]);
    eq(items.map((it) => it.state), ["waiting", "idle", "live", "off"]);
  });
  test("a channel stream counts the destinations that send it, and only those switched on", () => {
    const main = items.find((it) => it.key === "church/main");
    eq([main.sending, main.total, main.failing], [2, 3, 1]);
    eq(m.sendingText(main), "2 of 3 sending");
    eq(main.problems, ["Facebook: the far end closed the connection"]);
    eq(main.kbps, 4628);
    eq(m.streamFormat(main), "1920×1080 · 30 fps · H.264");
    const backup = items.find((it) => it.key === "church/backup");
    eq(m.sendingText(backup), "0 of 1 sending");
    eq(m.sendingText(items[0]), "No destinations");
  });
  test("a failing destination puts the stream in alarm; one waiting for an encoder is a warning", () => {
    eq(items.map(m.channelHealth), ["warning", "warning", "alarm", "off"]);
    eq(items.map(m.channelPictured), [false, false, true, false], "only a live stream has a picture to ask for");
  });
  test("a destination still dialling that has been told why not counts as failing; one only dialling does not", () => {
    const gym = { channels: [{ id: "gym", name: "Gym", enabled: true, streams: [{ name: "main", state: "live" }], destinations: [
      { id: "cdn", label: "Backup CDN", stream: "*", enabled: true, state: "connecting", error: "nothing answered at rtmp://127.0.0.1:19999" },
      { id: "yt", label: "YouTube", stream: "*", enabled: true, state: "connecting", error: null },
    ] }] };
    const [it] = m.channelItems(gym);
    eq([it.sending, it.total, it.failing], [0, 2, 1]);
    eq(it.problems, ["Backup CDN: nothing answered at rtmp://127.0.0.1:19999"]);
  });
  test("the Channels group has its band and obeys the wall's filter and alarm choice", () => {
    const all = m.channelGroup(items, {});
    eq([all[0].kind, all[0].label, all[0].count], ["group", "Channels", 4]);
    eq(m.channelGroup(items, { text: "church main" }).slice(1).map((it) => it.key), ["church/main"]);
    eq(m.channelGroup(items, { text: "facebook" }).slice(1).map((it) => it.key), ["church/main"], "a failing destination's words are searched");
    eq(m.channelGroup(items, { alarm: "any" }).slice(1).map((it) => it.key), ["church/main"]);
    eq(m.channelGroup(items, { alarm: "black" }), [], "no channel has a black alarm");
    eq(m.channelGroup([], {}), [], "no channels, no band");
  });

  const { channelRow, channelTile } = await import("../panels/wall/channel-rows.js");
  const { Thumbs } = await import("../panels/wall/thumbs.js");
  const { lines } = await import("../panels/wall/rows.js");
  const { channelThumbUrl } = await import("../panels/channels/picture.js");
  const thumbs = new Thumbs({ transport: {} }, 160, () => "about:blank");
  thumbs.stop();
  const ctx = { chanThumbs: thumbs };
  test("a channel row says how many destinations are sending, in red while one fails", () => {
    const row = channelRow(items.find((it) => it.key === "church/main"), ctx);
    const chip = row.querySelector(".wl-sending");
    eq(chip.textContent, "2 of 3 sending");
    ok(chip.classList.contains("failed"), "a retrying destination makes the chip red");
    ok(row.classList.contains("alarm"), "and the row");
    ok(row.textContent.includes("1920×1080 · 30 fps · H.264"), row.textContent);
    ok(row.textContent.includes("4.6 Mb/s"), row.textContent);
    eq(row.dataset.channel, "church/main");
    const calm = channelRow(items.find((it) => it.key === "church/backup"), ctx);
    ok(!calm.querySelector(".wl-sending").classList.contains("failed"));
    ok(calm.querySelector(".wl-picstate").textContent.includes("idle"));
  });
  test("channel streams go into lines of tiles as themselves, after their band", () => {
    const group = m.channelGroup(items, {});
    const l = lines(group, 3);
    eq(l.map((x) => x.kind), ["group", "line", "line"]);
    eq(l[1].shows.map((s) => s.key), ["annex", "church/backup", "church/main"]);
    const tile = channelTile(l[1].shows[2], ctx);
    eq(tile.querySelector(".wl-sending").textContent, "2 of 3 sending");
  });
  test("a channel stream's picture is asked for at its own route, with the token", () => {
    const u = new URL(channelThumbUrl({ transport: { base: "http://127.0.0.1:18670", token: "tok" } }, "church", "main", 320, 7));
    eq(u.pathname, "/api/v1/channels/church/streams/main/thumbnail.jpg");
    eq([u.searchParams.get("width"), u.searchParams.get("token"), u.searchParams.get("t")], ["320", "tok", "7"]);
  });
}
