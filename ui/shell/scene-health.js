// A scene's sources not running (absent or failed). The rest: scenes/fix-note.js.

export function notRunning(client, ids) {
  if (!client || !client.store || !((client.state || {}).sources || []).length) return [];
  return (ids || []).filter((id) => {
    const s = client.store.source(id);
    return !s || s.state === "failed";
  });
}

export function sceneNotRunning(client, scenes, id) {
  const summary = scenes && id ? scenes.summary(id) : null;
  return summary ? notRunning(client, summary.sources) : [];
}
