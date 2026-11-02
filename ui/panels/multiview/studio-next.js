// What Studio mode's preview holds, so it is never just black.
//
// After a take, Preview keeps the scene that was just taken, the same as
// Programme, until the operator picks another (`afterTake` "same", the
// default). In order: a source armed on this page, the armed scene, the
// scene on air, the source on air, and with nothing on air the first scene.
// `studio-armed.js` arms the scene a take put on air, so the core draws it
// with its filters, as it draws every armed scene.
//
// The old way is `afterTake` "swap", a choice in Settings. In order:
//
// 1. A scene armed on the core (scene.preview.set), unless it is the one on
//    air already. The scene on air is held too when somebody on this page
//    picked it since the last take: they chose it, so Take sends it.
// 2. A source armed on this page, unless it is the one on air already.
// 3. Otherwise a suggestion, the scene most likely to be taken next: the last
//    scene that was on air before the one on air now, or with none, the first
//    scene that is not on air. It is this page's guess and nobody else's: it
//    is not armed on the core, and the label under PREVIEW says so.
//
// So after a take, from this page or from anyone else (an agent, another
// operator), Preview shows where the show came from, as a vision mixer does
// when it swaps its buses. A suggestion is drawn on the page from the camera
// tiles, so an item's filters are not in it until it is armed.

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
 *
 * `chosen` is the scene picked on this page since the last take, if any. An
 * armed scene that is on air is still what Preview holds when it is that one,
 * because the person asked for it; after a take it is a suggestion again.
 */
export function nextUp(panel, s, kit, armedSource, chosen = null, afterTake = "same") {
  if (afterTake !== "swap") return sameUp(s, kit, armedSource);
  const live = liveScene(s, kit);
  const armed = armedScene(s, kit);
  if (armed && (idOf(kit, armed) !== live || (chosen && idOf(kit, chosen) === live))) {
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

/** Preview after a take keeps what was taken. See the top of this file. */
function sameUp(s, kit, armedSource) {
  if (armedSource) {
    const known = (s.sources || []).find((x) => x.id === armedSource);
    return { kind: "source", id: armedSource, name: (known && known.name) || armedSource, why: "armed" };
  }
  const armed = armedScene(s, kit);
  if (armed) return { kind: "scene", id: idOf(kit, armed), name: armed, why: "armed" };
  const live = liveScene(s, kit);
  const summary = live && kit ? kit.summary(live) : null;
  if (summary) return { kind: "scene", id: summary.id, name: summary.name, why: "live" };
  if (s.program) {
    const known = (s.sources || []).find((x) => x.id === s.program);
    return { kind: "source", id: s.program, name: (known && known.name) || s.program, why: "live" };
  }
  const first = kit ? kit.scenes()[0] : null;
  return first ? { kind: "scene", id: first.id, name: first.name, why: "next" } : null;
}

/** The few words under PREVIEW that say why it holds what it holds. */
export function whyText(next) {
  if (!next || next.why === "armed" || next.why === "live") return "";
  return next.why === "before" ? "Suggested: on air before this" : "Suggested: next scene";
}
