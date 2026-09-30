// Whether a setting is waiting for a restart, asked on every connect. The
// bar itself (restart-bar.js) is fetched only once one is: most sessions
// never have one. Once here it is asked on every reconnect, which is how it
// goes away after the restart.

let bar = null;

export function restartBar(client) {
  const check = async () => {
    if (!bar) {
      const c = await client.call("config.get", {}).catch(() => ({}));
      if (!(c.needs_restart || []).length) return;
      bar = import("./restart-bar.js");
    }
    (await bar).checkRestart(client);
  };
  client.on("open", check);
  check();
}
