import { openSceneSources } from '../panels/sources/chooser.js';
import { acquireScenes } from '../shell/scene-session.js';

export async function sourceChooserTests(test, eq, ok) {
  const listeners = new Set();
  let wants = 0, releases = 0, attached = 0, detached = 0;
  const client = {
    state: { sources: [{ id: 'cam', name: 'Camera', cell: 0, has_video: true }], multiview: { enabled: true, cols: 1 } },
    call: async method => method === 'plugin.list' ? { plugins: [] } : method === 'device.discover' ? { candidates: [] } : method === 'media.list' ? { items: [] } : {},
    onRender(fn) { listeners.add(fn); return () => listeners.delete(fn); },
    want() { wants++; return { update() {}, release() { releases++; } }; },
    sheet: { attach() { attached++; return () => detached++; } },
  };
  const calls = [];
  const scenes = { itemAdd: async (...args) => calls.push(args), reread: async () => {}, undo: { record() {} } };
  const dialog = await openSceneSources(client, scenes, { id: 'wide', name: 'Wide', sources: [] });
  [...dialog.el.querySelectorAll('[role="tab"]')].find(tab => tab.textContent.includes('Existing sources')).click();
  test('opening the source library requests no preview work', () => eq(wants, 0));
  dialog.el.querySelector('[aria-label="Preview Camera"]').click();
  test('preview is explicit and attaches only the selected source', () => { eq(wants, 1); eq(attached, 1); });
  const picture = dialog.el.querySelector('canvas');
  picture.getContext('2d').fillRect(0, 0, 10, 10);
  client.state.sources.push({ id: 'pending', name: 'Connecting', cell: null, has_video: true });
  for (const listener of listeners) listener(client.state);
  dialog.el.querySelector('[aria-label="Preview Connecting"]').click();
  test('a connecting source never displays the previous source picture', () => {
    ok(picture.hidden); eq(picture.getContext('2d').getImageData(0, 0, 1, 1).data[3], 0);
  });
  dialog.close();
  test('closing the source chooser releases its preview and listeners', () => { eq(releases, 2); eq(detached, 1); eq(listeners.size, 0); eq(calls.length, 0); });

  const events = new Set();
  const sceneClient = { call: async method => method === 'scene.list' ? [] : {}, on(name, fn) { events.add(fn); return () => events.delete(fn); } };
  const first = acquireScenes(sceneClient);
  const second = acquireScenes(sceneClient);
  await first.ready;
  test('scene docks share one scene client', () => ok(first.scenes === second.scenes));
  first.release();
  test('closing one scene dock leaves the shared model subscribed', () => eq(events.size, 1));
  second.release();
  test('closing the last scene dock releases the shared model', () => eq(events.size, 0));
}
