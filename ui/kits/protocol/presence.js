// Who else is operating this mixer, read off `presence.list` and
// `event/presence.changed`.
//
// Pure functions over the list the core sends, so the shell's indicator and
// the composer's marker agree, and so both can be tested without a page.

/** Everybody but this connection. */
export function others(list) {
  return ((list && list.clients) || []).filter((c) => !c.you);
}

/** The others who said they are editing this scene, by id. */
export function editing(list, sceneId) {
  if (!sceneId) return [];
  return others(list).filter((c) => c.scene === sceneId);
}

/**
 * How to name a client to a person: the name it gave itself or its token's
 * label, its device, and the client id only when there is nothing better.
 */
export function nameOf(client) {
  if (!client) return "somebody";
  const label = client.label && String(client.label).trim();
  const device = client.device && String(client.device).trim();
  if (label && device) return `${label} (${device})`;
  return label || device || client.client_id || "somebody";
}

/** "Sam's phone", "Sam's phone and an iPhone", "3 others". */
export function sentence(clients) {
  const names = (clients || []).map(nameOf);
  if (names.length === 0) return "";
  if (names.length === 1) return names[0];
  if (names.length === 2) return `${names[0]} and ${names[1]}`;
  return `${names.length} others`;
}
