import { monitorResizeTests } from './monitor-resize.js';
// Exercise the studio controls against the public request shape.
export async function studioTests(test, eq, ok) {
  const { lazyAction } = await import('../shell/lazy-action.js');
  let release, opened = 0;
  const load = new Promise(resolve => { release = resolve; });
  const open = lazyAction(() => load, 'Open test dialog');
  const first = open();
  const second = open();
  release(() => ++opened);
  await Promise.all([first, second]);
  test('repeated clicks during a lazy dialog download open one dialog', () => eq(opened, 1));
  await open();
  test('a completed lazy action can be opened again', () => eq(opened, 2));
  window.godwinmixPanels ||= [];
  const { default: ProgramPanel } = await import('../panels/multiview/panel.js');
  await monitorResizeTests(test, eq, ok, ProgramPanel);
  const panel = new ProgramPanel();
  const calls = [];
  panel.setClient({ state: { preview: 'wide-scene' }, call: async (method, params) => calls.push({ method, params }) });
  panel.armed = 'wide-scene';
  await panel.take(500);
  test('studio Fade takes a scene with the selected duration inside transition', () => {
    eq(calls[0], { method: 'program.take', params: { scene: 'wide-scene', transition: { type: 'fade', duration_ms: 500 } } });
  });
  panel.client.state.preview = null;
  panel.armed = 'camera';
  await panel.take(0);
  test('studio Cut takes an armed source without a transition', () => {
    eq(calls[1], { method: 'program.take', params: { source: 'camera' } });
  });
  let released = 0;
  panel.want = { release() { released++; } };
  panel.previewWant = { release() { released++; } };
  panel.setWorkspaceActive(false);
  test('a hidden programme panel releases both preview subscriptions', () => {
    eq(released, 2);
    ok(panel.want === null && panel.previewWant === null);
  });
}
