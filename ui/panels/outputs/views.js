// The parts of the Outputs panel that only a session with renditions uses:
// the Resources tab, the planner's line under each row, and the playback
// card under an HLS output. Fetched the first time any of them is wanted,
// so the panel the page loads stays the size it was.

/** Switch the panel between its destinations and Resources. */
export async function show(panel, view) {
  panel.view = view;
  const tabs = [...panel.tabs.children];
  tabs.forEach((b, i) => b.setAttribute("aria-selected", String(i === (view === "resources" ? 1 : 0))));
  panel.list.hidden = view === "resources";
  if (view !== "resources") {
    if (panel.resources) {
      panel.resources.stop();
      panel.resources.node.hidden = true;
    }
    return panel.render(panel.client.state);
  }
  panel.follow(false);
  followPlan(panel, false);
  if (!panel.resources) {
    const { ResourcesView } = await import("../renditions/resources.js");
    panel.resources ||= new ResourcesView(panel.client);
    panel.appendChild(panel.resources.node);
  }
  if (panel.view !== "resources") return;
  panel.resources.node.hidden = false;
  if (panel.workspaceActive !== false) panel.resources.start();
}

/** Stop everything this module started, when the panel goes off screen. */
export function stopAll(panel) {
  followPlan(panel, false);
  if (panel.resources) panel.resources.stop();
}

/**
 * Keep `panel.planText(id)` answering what the planner did for each row,
 * asking for `rendition.*` events only while it is wanted.
 */
export async function followPlan(panel, wanted) {
  if (!wanted) {
    if (panel.unplan) panel.unplan();
    panel.unplan = null;
    return;
  }
  if (panel.unplan) return;
  let stopped = false;
  panel.unplan = () => (stopped = true);
  const feed = await import("../renditions/plan-feed.js");
  if (stopped) return;
  let plan = null;
  panel.planText = (id) => feed.planLine(plan, id);
  panel.unplan = feed.followPlan(panel.client, "programme", (next) => {
    plan = next;
    panel.render(panel.client.state);
  });
}

/** The HLS playback card, put into `slot` and kept to the row's status. */
export async function hlsInto(client, slot, current) {
  const { hlsCard } = await import("../renditions/hls-card.js");
  const card = hlsCard(client, current());
  slot.appendChild(card.node);
  slot.update = (next) => card.update(next);
  card.update(current());
}
