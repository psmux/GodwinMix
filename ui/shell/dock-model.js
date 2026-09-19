// A serializable split tree. Panel elements never belong to the tree.
export const KEY = 'gmx.workspace.v1';
export const leaf = (ids) => ({ tabs: ids, active: ids[0] });
export function leaves(tree) {
  return tree?.tabs ? [tree] : tree ? [...leaves(tree.a), ...leaves(tree.b)] : [];
}
export function clean(tree, seen = new Set(), depth = 0) {
  if (!tree || depth > 24) return null;
  if (Array.isArray(tree.tabs)) {
    const tabs = tree.tabs.filter(id => typeof id === 'string' && !seen.has(id) && seen.add(id));
    return tabs.length ? { tabs, active: tabs.includes(tree.active) ? tree.active : tabs[0] } : null;
  }
  const a = clean(tree.a, seen, depth + 1), b = clean(tree.b, seen, depth + 1);
  if (!a || !b) return a || b;
  return { axis: tree.axis === 'y' ? 'y' : 'x', ratio: Math.max(.15, Math.min(.85, Number(tree.ratio) || .5)), a, b };
}
export function initial(layout) {
  const ids = [...new Set(Object.entries(layout).filter(([s]) => !['header', 'modal'].includes(s)).flatMap(([, v]) => v))];
  const program = ids.includes('core/program') ? ['core/program'] : [];
  const scenes = ids.filter(id => ['core/scenes', 'core/sources'].includes(id));
  const other = ids.filter(id => !program.includes(id) && !scenes.includes(id));
  const controls = scenes.length > 1 ? { axis: 'x', ratio: .5, a: leaf([scenes[0]]), b: leaf(scenes.slice(1)) } : leaf(scenes);
  const center = program.length ? { axis: 'y', ratio: .65, a: leaf(program), b: controls } : controls;
  return clean({ axis: 'x', ratio: .72, a: center, b: leaf(other) });
}
export function load(layout) {
  try {
    const value = JSON.parse(localStorage.getItem(KEY));
    if (value?.version === 1) {
      const tree = clean(value.tree), active = new Set(leaves(tree).flatMap(n => n.tabs));
      const hidden = Array.isArray(value.hidden) ? [...new Set(value.hidden.filter(x => typeof x === 'string' && !active.has(x)))] : [];
      return { tree, hidden };
    }
  } catch { /* Storage is optional. */ }
  return { tree: initial(layout), hidden: [] };
}
export function save(state) {
  try { localStorage.setItem(KEY, JSON.stringify({ version: 1, ...state })); } catch { /* Session layout still works. */ }
}
export function remove(tree, id) {
  if (!tree) return null;
  if (tree.tabs) return clean({ ...tree, tabs: tree.tabs.filter(x => x !== id) });
  return clean({ ...tree, a: remove(tree.a, id), b: remove(tree.b, id) });
}
export function dock(tree, id, targetId, edge) {
  if (id === targetId || !leaves(tree).some(n => n.tabs.includes(targetId))) return tree;
  if (!['left', 'right', 'top', 'bottom', 'center'].includes(edge)) return tree;
  const next = remove(tree, id);
  if (!next) return leaf([id]);
  function insert(node) {
    if (!node.tabs) return { ...node, a: insert(node.a), b: insert(node.b) };
    if (!node.tabs.includes(targetId)) return node;
    if (edge === 'center') return { tabs: [...node.tabs, id], active: id };
    const before = edge === 'left' || edge === 'top';
    return { axis: ['left', 'right'].includes(edge) ? 'x' : 'y', ratio: .5, a: before ? leaf([id]) : node, b: before ? node : leaf([id]) };
  }
  return insert(next);
}
export function rectangles(tree, box, panels = [], splits = []) {
  if (!tree) return { panels, splits };
  if (tree.tabs) panels.push({ node: tree, ...box });
  else {
    const x = tree.axis === 'x', size = x ? box.w : box.h, first = (size - 6) * tree.ratio;
    rectangles(tree.a, { ...box, [x ? 'w' : 'h']: first }, panels, splits);
    const divider = { ...box, [x ? 'x' : 'y']: box[x ? 'x' : 'y'] + first, [x ? 'w' : 'h']: 6 };
    splits.push({ node: tree, box, ...divider });
    rectangles(tree.b, { ...box, [x ? 'x' : 'y']: box[x ? 'x' : 'y'] + first + 6, [x ? 'w' : 'h']: size - first - 6 }, panels, splits);
  }
  return { panels, splits };
}
