/** Include sources in groups and referenced scenes, without following cycles. */
export function sourceMembership(mirror, scene) {
  const found = new Set(), visited = new Set();
  function visit(id) {
    if (visited.has(id)) return;
    visited.add(id);
    for (const record of mirror.descendants(id)) {
      const content = record.content || {};
      if (content.source) found.add(content.source);
      if (content.ref) visit(content.ref);
    }
  }
  visit(scene);
  return [...found];
}
