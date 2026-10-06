// What Studio mode's preview holds, so it is never just black.
//
// In order:
//
// 1. A scene armed on the core (scene.preview.set), unless it is the one on
//    air already.
// 2. A source armed on this page, unless it is the one on air already.
// 3. Otherwise a suggestion, the scene most likely to be taken next: the last
//    scene that was on air before the one on air now, or with none, the first
//    scene that is not on air. It is this page's guess and nobody else's: it
//    is not armed on the core, and the label under PREVIEW says so.
//
// So after a take, from this page or from anyone else (an agent, another
// operator), Preview shows where the show came from rather than the same
// shot twice, which is what a vision mixer does when it swaps its buses.

/** The scene list, from this page's scene session or the Scenes panel. */
export function sceneKit(panel) {
  const own = panel && panel.sceneSession && panel.sceneSession.scenes;
  if (own && own.supported) return own;
  const node = typeof document !== "undefined" && document.querySelector("gmx-scenes");
  return node && node.scenes && node.scenes.supported ? node.scenes : null;
}

/** A scene's id from its id or its name; the core uses both. */
function idOf(kit, scene) {
  if (!scene) return null;
  const summary = kit && kit.summary(scene);
  return summary ? summary.id : scene;
}

/** The armed scene's name: the event's, else the scene list's flag. */
export function armedScene(s, kit) {
  if (s.preview) return s.preview;
  const id = kit && kit.armed();
  const summary = id && kit.summary(id);
  return summary ? summary.name : null;
}

/** The scene on air, as an id, or null when a source or black is. */
export function liveScene(s, kit) {
  const live = kit ? kit.live() : null;
  return live || idOf(kit, s.scene || null);
}

/**
 * Remember where the show has been: the scenes that were on air, newest
 * first, kept on the panel. Called on every render; does nothing until the
 * scene on air changes.
 */
export function follow(panel, s, kit) {
  const live = liveScene(s, kit);
  const key = live || (s.program ? `source:${s.program}` : "black");
  if (panel.onAirKey === undefined) {
    panel.onAirKey = key;
    panel.onAirLive = live;
    panel.onAirBefore = panel.onAirBefore || [];
    return;
  }
  if (key === panel.onAirKey) return;
  const was = panel.onAirLive;
  panel.onAirKey = key;
  panel.onAirLive = live;
  if (was) panel.onAirBefore = [was, ...panel.onAirBefore.filter((x) => x !== was)].slice(0, 8);
}

/**
 * What Preview shows: `{kind: "scene"|"source", id, name, why}`, where why is
 * "armed", "before" (was on air before) or "next" (the first other scene),
 * or null when there is nothing at all to show.
 */
export function nextUp(panel, s, kit, armedSource) {
  const live = liveScene(s, kit);
  const armed = armedScene(s, kit);
  if (armed && idOf(kit, armed) !== live) {
    return { kind: "scene", id: idOf(kit, armed), name: armed, why: "armed" };
  }
  if (armedSource && !(s.program === armedSource && !s.scene)) {
    const known = (s.sources || []).find((x) => x.id === armedSource);
    return { kind: "source", id: armedSource, name: (known && known.name) || armedSource, why: "armed" };
  }
  const scenes = kit ? kit.scenes() : [];
  const exists = (id) => scenes.find((x) => x.id === id);
  const before = (panel.onAirBefore || []).find((id) => id !== live && exists(id));
  const pick = before ? exists(before) : scenes.find((x) => x.id !== live);
  if (!pick) return null;
  return { kind: "scene", id: pick.id, name: pick.name, why: before ? "before" : "next" };
}

/** The few words under PREVIEW that say why it holds what it holds. */
export function whyText(next) {
  if (!next || next.why === "armed") return "";
  return next.why === "before" ? "Suggested: on air before this" : "Suggested: next scene";
}
