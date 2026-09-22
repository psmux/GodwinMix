import { SceneClient } from '../kits/protocol/index.js';

const sessions = new WeakMap();

/**
 * The name the scene on air goes by now, written where the whole page can see it.
 *
 * The core reports that scene by the name it had when it was taken and never
 * revises it, so a rename left the header, and anything else rendering on the
 * core's flush, reading the old one. The scene document is the only thing that
 * knows better, so the one session that holds it files the answer in the store
 * and every panel picks it up on the next flush. A mixer with no scene server
 * never gets here and `sceneName` stays null.
 */
function publishLiveName(client, scenes) {
  const summary = scenes.summary(scenes.live());
  const name = summary ? summary.name : null;
  if (name === client.store.state.sceneName) return;
  client.store.patch({ sceneName: name });
  client.store.flush();
}

/** Scene data belongs to the client, so closing one dock cannot remove it. */
export function acquireScenes(client, undo) {
  let session = sessions.get(client);
  if (!session) {
    const scenes = new SceneClient(client, { undo });
    session = { scenes, refs: 0 };
    session.offName = scenes.onChange(() => publishLiveName(client, scenes));
    const started = session;
    session.ready = scenes.start().then(() => { if (!started.refs) scenes.stop(); return scenes; });
    sessions.set(client, session);
  }
  session.refs++;
  let released = false;
  return { scenes: session.scenes, ready: session.ready, release() {
    if (released) return;
    released = true;
    if (--session.refs === 0) {
      session.offName();
      session.scenes.stop();
      sessions.delete(client);
    }
  } };
}
