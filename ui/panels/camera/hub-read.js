// The reading behind hub.js: one clock of 125 ms that runs only while
// something wants a topic and the page is not hidden. Each tick reads what is
// due: the plugin list and the camera and switcher lists every five seconds,
// tracking and switcher state twice a second, and the people one camera sees
// every tick for each camera somebody is looking at.

const SLOW = 40;
const MID = 4;

export function reader(hub) {
  let timer = null;
  let n = 0;
  let busy = null;

  async function lists() {
    await hub.plugins();
    const [cams, atems] = await Promise.all([
      hub.cameras && (hub.wanted("cams") || hub.wanted("people")) ? hub.api.cameras().catch(() => hub.cams) : hub.cams,
      hub.switchers && hub.wanted("atem") ? hub.api.atems() : hub.atems,
    ]);
    hub.cams = hub.cameras ? cams : [];
    hub.atems = hub.switchers ? atems : [];
    hub.loaded = true;
    hub.changed("lists");
  }

  async function states() {
    if (hub.wanted("cams") || hub.wanted("people")) {
      const ask = hub.wanted("cams") ? hub.cams : hub.cams.filter((c) => hub.wants.has("people:" + c.id));
      const got = await Promise.all(ask.map((c) => hub.api.trackState(c.id).catch(() => null)));
      ask.forEach((c, i) => hub.track.set(c.id, got[i]));
    }
    if (hub.wanted("atem")) {
      const up = hub.atems.filter((a) => a.connected !== false);
      const got = await Promise.all(up.map((a) => hub.api.atemState(a.id).catch(() => null)));
      hub.atemState = new Map(up.map((a, i) => [a.id, got[i]]));
    }
    hub.changed("state");
  }

  async function people() {
    const ids = [...hub.wants.keys()].filter((k) => k.startsWith("people:")).map((k) => k.slice(7));
    if (!ids.length || !hub.cameras) return;
    const got = await Promise.all(ids.map((id) => (hub.cam(id) ? hub.api.observe(id).catch(() => null) : null)));
    ids.forEach((id, i) => hub.seen.set(id, got[i]));
    hub.changed("people");
  }

  async function tick(all) {
    // A slow plugin must not stack reads up behind itself.
    if (busy) return busy;
    if (document.hidden && !all) return;
    const i = n++;
    busy = (async () => {
      try {
        if (all || i % SLOW === 0) await lists();
        if (all || i % MID === 0) await states();
        await people();
      } catch (e) {
        if (!hub.error) console.warn("the camera controls could not read", e);
        hub.error = e;
      } finally {
        busy = null;
      }
    })();
    return busy;
  }

  return {
    start() {
      if (timer) return;
      n = 0;
      timer = setInterval(() => tick(false), 125);
      hub.demo?.start();
      tick(false);
    },
    stop() {
      clearInterval(timer);
      timer = null;
      hub.demo?.stop();
    },
    /** Everything, now: after a command, the demo switched, or a drawer opened. */
    async now() {
      await busy;
      return tick(true);
    },
  };
}
