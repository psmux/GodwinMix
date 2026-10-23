// A headend station runs no mixer, so its page opens on the headend view:
// its own core.info says `station` and `headend` (station/alone.rs).
export async function openIfHeadend(client) {
  const f = (await client.call("core.info", {}).catch(() => null))?.features || [];
  if (!f.includes("station") || !f.includes("headend")) return false;
  (await import("./view.js")).showHeadend(client);
  return true;
}
