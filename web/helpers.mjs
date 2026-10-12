export const escapeHtml = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export const memory = (bytes) => bytes ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : '—';
export const date = (seconds) => new Date(seconds * 1000).toLocaleString('en-US');
export function imageChoices(profiles, records, defaultImage, selected) {
  const choices = new Map();
  for (const item of profiles) choices.set(item.profile.reference, { value: item.profile.reference, label: item.profile.name, disabled: !item.available });
  if (defaultImage && !choices.has(defaultImage)) choices.set(defaultImage, { value: defaultImage, label: defaultImage === 'wsl-desktop/ubuntu:24.04' ? 'Ubuntu 24.04 · Lightweight XFCE (legacy)' : defaultImage, disabled: false });
  for (const record of records) choices.set(record.image_id, { value: record.image_id, label: record.desktop ? `${record.name} — ${record.desktop.title}` : record.name, disabled: record.available === false });
  if (selected && !choices.has(selected)) choices.set(selected, { value: selected, label: selected, disabled: false });
  return [...choices.values()];
}
export function imageTree(records) {
  const ids = new Set(records.map(r => r.image_id));
  const parents = new Map();
  for (const record of records) {
    const parent = record.parent_image_id || null;
    if (!parents.has(parent)) parents.set(parent, []);
    parents.get(parent).push(record);
  }
  const result = [], seen = new Set();
  function visit(record, depth) {
    const stack = [[record, depth]];
    while (stack.length) {
      const [item, level] = stack.pop();
      if (seen.has(item.id)) continue;
      seen.add(item.id); result.push({ ...item, depth: level });
      const children = parents.get(item.image_id) || [];
      for (let i = children.length - 1; i >= 0; i--) stack.push([children[i], level + 1]);
    }
  }
  for (const r of records) if (!r.parent_image_id || !ids.has(r.parent_image_id)) visit(r, 0);
  // Corrupt/cyclic metadata must not hide records or hang the UI.
  for (const r of records) if (!seen.has(r.id)) visit(r, 0);
  return result;
}

