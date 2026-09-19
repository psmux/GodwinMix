import { SceneClient } from '../kits/protocol/index.js';

const sessions = new WeakMap();

/** Scene data belongs to the client, so closing one dock cannot remove it. */
export function acquireScenes(client, undo) {
  let session = sessions.get(client);
  if (!session) {
    const scenes = new SceneClient(client, { undo });
    session = { scenes, refs: 0 };
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
      session.scenes.stop();
      sessions.delete(client);
    }
  } };
}
