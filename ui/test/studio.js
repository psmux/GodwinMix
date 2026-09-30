import { monitorResizeTests } from './monitor-resize.js';
import { DEFAULT_MAP } from '../shell/keymap.js';
import { settings, setSetting } from '../shell/settings.js';
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
  const studio = await import('../panels/multiview/studio.js');
  await monitorResizeTests(test, eq, ok, ProgramPanel);
  const panel = new ProgramPanel();
  panel.studio = studio;
  const calls = [];
  panel.setClient({ state: { preview: 'wide-scene' }, call: async (method, params) => calls.push({ method, params }) });
  panel.armed = 'wide-scene';
  await panel.take(500);
  test('studio Take sends a scene with the chosen transition and length', () => {
    eq(calls[0], { method: 'program.take', params: { scene: 'wide-scene', transition: { type: 'fade', duration_ms: 500 } } });
  });
  panel.client.state.preview = null;
  panel.armed = 'camera';
  await panel.take(0);
  test('studio Cut takes an armed source without a transition', () => {
    eq(calls[1], { method: 'program.take', params: { source: 'camera' } });
  });

  // The preview holds one thing: the armed scene, or a source armed here.
  const armedBefore = document.body.dataset.armed;
  document.body.dataset.armed = 'cam-wide';
  studio.render(panel, { preview: null });
  test('a source armed on the page is what the preview holds', () => eq(panel.armed, 'cam-wide'));
  studio.render(panel, { preview: 'Two shot' });
  test('arming a scene after a source clears the source', () => {
    eq(panel.armed, 'Two shot');
    ok(!document.body.dataset.armed, 'the source stayed armed under the scene');
  });
  if (armedBefore === undefined) delete document.body.dataset.armed; else document.body.dataset.armed = armedBefore;

  // A source in preview is drawn from its own mosaic tile, and asks the core
  // for no preview stream, which only a scene has.
  const producer = settings().producer;
  const attached = [], wanted = [];
  const pane = new ProgramPanel();
  pane.studio = studio;
  pane.visible = true;
  pane.previewCanvas = document.createElement('canvas');
  pane.setClient({
    state: {},
    sheet: { attach: (canvas, cell) => { attached.push(cell); return () => attached.push('off'); } },
    preview: { attach: () => () => {} },
    want: kind => { wanted.push(kind); return { update() {}, release() {} }; },
  });
  setSetting('producer', true);
  document.body.dataset.armed = 'cam-wide';
  studio.retunePreview(pane, { preview: null, multiview: { cells: [{ source: null, index: 0 }, { source: 'cam-wide', index: 2 }] } });
  test('a source in preview is its own tile out of the mosaic', () => {
    eq(attached, [2]);
    eq(wanted, []);
  });
  studio.retunePreview(pane, { preview: 'Two shot', multiview: { cells: [] } });
  test('a scene armed after it lets go of the tile and asks for the preview stream', () => {
    eq(attached, [2, 'off']);
    eq(wanted, ['preview']);
  });
  studio.releasePreview(pane);
  setSetting('producer', producer);
  if (armedBefore === undefined) delete document.body.dataset.armed; else document.body.dataset.armed = armedBefore;

  // A click on a source tile inside a scene selects it, and in Studio mode
  // arms it too: before, it did nothing a person could see.
  const { default: SourcesPanel } = await import('../panels/sources/panel.js');
  const trayCalls = [];
  let heard = 0;
  const hear = () => heard++;
  document.addEventListener('gmx-armed', hear);
  const tray = {
    tiles: new Map(),
    scopedTo: () => ({ id: 'wide' }),
    putOnAir: SourcesPanel.prototype.putOnAir,
    sceneClient: () => null,
    render() {},
    client: { state: { preview: 'Two shot' }, call: async (method, params) => trayCalls.push({ method, params }) },
  };
  setSetting('producer', true);
  SourcesPanel.prototype.activate.call(tray, 'cam-wide');
  await new Promise(resolve => setTimeout(resolve, 0));
  test('in Studio mode a source tile inside a scene is put in preview', () => {
    eq(document.body.dataset.armed, 'cam-wide');
    eq(trayCalls, [{ method: 'scene.preview.set', params: {} }]);
    eq(heard, 1);
  });
  setSetting('producer', false);
  delete document.body.dataset.armed;
  SourcesPanel.prototype.activate.call(tray, 'cam-wide');
  test('outside Studio mode the same click only selects, as before', () => {
    ok(!document.body.dataset.armed);
    eq(trayCalls.length, 1);
  });
  document.removeEventListener('gmx-armed', hear);
  setSetting('producer', producer);
  if (armedBefore === undefined) delete document.body.dataset.armed; else document.body.dataset.armed = armedBefore;

  test('Space sends preview to programme, and does not take Enter or the number keys', () => {
    eq(DEFAULT_MAP.Space, 'program.take-armed');
    eq(DEFAULT_MAP.Enter, 'tray.open');
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
