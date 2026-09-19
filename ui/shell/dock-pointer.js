import { el } from './dom.js';
import { place } from './dock.js';
import { save } from './dock-model.js';

function track(event, move, finish) {
  const abort = new AbortController();
  const options = { signal: abort.signal };
  const end = e => {
    if (e.pointerId !== undefined && e.pointerId !== event.pointerId) return;
    abort.abort(); finish(e.type === 'pointerup', e);
  };
  window.addEventListener('pointermove', e => { if (e.pointerId === event.pointerId) move(e); }, options);
  window.addEventListener('pointerup', end, options);
  window.addEventListener('pointercancel', end, options);
  window.addEventListener('blur', end, options);
  window.addEventListener('keydown', e => { if (e.key === 'Escape') end(e); }, options);
  event.preventDefault();
}
export function gestures(workspace, handle, id) {
  handle.addEventListener('pointerdown', start => {
    if (start.button !== 0 || workspace.root.clientWidth <= 760) return;
    const overlay = el('div.dock-target', { 'aria-hidden': 'true' });
    let target = null, edge = null, dragging = false, pending = 0;
    function update(e) {
      if (Math.hypot(e.clientX - start.clientX, e.clientY - start.clientY) < 6 && !dragging) return;
      dragging = true;
      document.body.classList.add('dock-dragging');
      if (!overlay.isConnected) workspace.root.append(overlay);
      target = null;
      for (const [other, frame] of workspace.frames) {
        if (other === id) continue;
        const r = frame.element.getBoundingClientRect();
        if (e.clientX < r.left || e.clientX > r.right || e.clientY < r.top || e.clientY > r.bottom) continue;
        target = other;
        const x = (e.clientX - r.left) / r.width, y = (e.clientY - r.top) / r.height;
        edge = x < .24 ? 'left' : x > .76 ? 'right' : y < .24 ? 'top' : y > .76 ? 'bottom' : 'center';
        const root = workspace.root.getBoundingClientRect();
        const box = { x: r.left - root.left, y: r.top - root.top, w: r.width, h: r.height };
        if (edge === 'left' || edge === 'right') { box.w /= 2; if (edge === 'right') box.x += box.w; }
        if (edge === 'top' || edge === 'bottom') { box.h /= 2; if (edge === 'bottom') box.y += box.h; }
        place(overlay, box);
        overlay.textContent = edge === 'center' ? 'Group as tabs' : 'Dock ' + edge;
      }
      overlay.hidden = !target;
    }
    track(start, e => {
      cancelAnimationFrame(pending);
      pending = requestAnimationFrame(() => update(e));
    }, (commit, e) => {
      cancelAnimationFrame(pending);
      if (commit) update(e);
      overlay.remove();
      document.body.classList.remove('dock-dragging');
      if (commit && target) workspace.move(id, target, edge);
    });
  });
}
export function splitter(workspace, rect) {
  const horizontal = rect.node.axis === 'x';
  const node = el('div.dock-splitter', { role: 'separator', tabindex: 0,
    'aria-label': 'Resize workspace panes', 'aria-orientation': horizontal ? 'vertical' : 'horizontal',
    'aria-valuemin': 15, 'aria-valuemax': 85, 'aria-valuenow': Math.round(rect.node.ratio * 100),
    'data-axis': rect.node.axis,
  });
  place(node, rect);
  workspace.root.append(node);
  const change = value => { rect.node.ratio = Math.max(.15, Math.min(.85, value)); workspace.schedule(); };
  node.addEventListener('keydown', e => {
    const delta = ['ArrowLeft', 'ArrowUp'].includes(e.key) ? -.025 : ['ArrowRight', 'ArrowDown'].includes(e.key) ? .025 : 0;
    if (!delta) return;
    e.preventDefault();
    change(rect.node.ratio + delta);
    save(workspace.state);
  });
  node.addEventListener('pointerdown', e => {
    if (e.button !== 0) return;
    const root = workspace.root.getBoundingClientRect(), original = rect.node.ratio;
    document.body.classList.add('dock-dragging');
    track(e, move => {
      const point = horizontal ? move.clientX - root.left - rect.box.x : move.clientY - root.top - rect.box.y;
      change(point / ((horizontal ? rect.box.w : rect.box.h) - 6));
    }, commit => {
      if (!commit) change(original);
      document.body.classList.remove('dock-dragging');
      save(workspace.state);
    });
  });
  return node;
}
