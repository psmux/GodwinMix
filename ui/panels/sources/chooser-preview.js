import { el, on } from '../../shell/dom.js';
import { sheetWidthFor } from '../../client/frames.js';
import { nameOf } from './local.js';

/** A single opt-in picture, with no request until Preview is pressed. */
export function sourcePreview(client) {
  const title = el('strong', { text: 'Preview a source before adding it' });
  const canvas = el('canvas', { width: 640, height: 360, hidden: true });
  const note = el('p.dim', { text: 'Choose Preview beside a source. This does not change the programme.' });
  const stop = el('button.btn', { text: 'Stop preview', hidden: true, onclick: () => select(null) });
  const node = el('section.source-chooser-preview', {}, [title, canvas, note, stop]);
  let selected = null, want = null, detach = null, cell = null, width = null;
  function release() {
    if (detach) detach();
    if (want) want.release();
    detach = want = null;
    cell = width = null;
  }
  function render(state) {
    const source = (state.sources || []).find(s => s.id === selected);
    if (!source || document.hidden || source.has_video === false || state.multiview?.enabled === false) {
      release();
      canvas.hidden = true;
      if (selected) note.textContent = !source ? 'This source is no longer available.' : source.has_video === false ? 'This is an audio source.' : 'Live preview is paused.';
      return;
    }
    const nextWidth = sheetWidthFor(320, state.multiview?.cols || 1);
    if (!want) want = client.want('multiview', { fps: 8, width: nextWidth });
    else if (nextWidth !== width) want.update({ fps: 8, width: nextWidth });
    width = nextWidth;
    if (source.cell !== cell || (!detach && source.cell != null)) {
      if (detach) detach();
      detach = source.cell == null ? null : client.sheet.attach(canvas, source.cell);
      cell = source.cell;
    }
    canvas.hidden = false;
    note.textContent = source.cell == null ? 'Waiting for a preview picture.' : 'Preview only. The programme is unchanged.';
  }
  function select(source) {
    release();
    selected = source?.id || null;
    title.textContent = source ? nameOf(source) : 'Preview a source before adding it';
    stop.hidden = !source;
    note.textContent = 'Choose Preview beside a source. This does not change the programme.';
    render(client.state);
  }
  const off = client.onRender(render);
  const offVisibility = on(document, 'visibilitychange', () => render(client.state));
  return { node, select, destroy() { off(); offVisibility(); release(); } };
}
