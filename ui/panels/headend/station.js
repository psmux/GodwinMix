// A headend station runs no mixer, so its page opens on the headend view.
//
// The station answers `core.info` itself while no show stands behind the
// page (`crates/godwinmix/src/station/alone.rs`), and says so with the
// feature `station`; `headend` says it follows a Flussonic config. With
// both, the mixer's panels have nothing to show and the channels are what
// the page is for.

export async function openIfHeadend(client) {
  const info = await client.call("core.info", {}).catch(() => null);
  const features = (info && info.features) || [];
  if (!features.includes("station") || !features.includes("headend")) return false;
  const { showHeadend } = await import("./view.js");
  showHeadend(client);
  return true;
}
