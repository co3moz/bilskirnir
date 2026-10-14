import { fileSize } from './uploads.mjs';

export const escapeHtml = (value) => String(value ?? '').replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
export const memory = (bytes) => bytes ? `${(bytes / 1024 ** 3).toFixed(1)} GB` : '—';
export const date = (seconds) => new Date(seconds * 1000).toLocaleString('en-US');
export function jobLogText(job) {
  if (job.log?.trim()) return job.log;
  if (job.status === 'queued') return 'Waiting to start…';
  if (job.status === 'running') return 'Operation in progress…';
  if (job.status === 'completed') {
    if (job.result?.deleted) {
      const name = job.result.name;
      return `${name ? `Deleted "${name}".` : 'Environment deleted.'}${job.result.volumes_preserved ? ' Named volumes were preserved.' : ''}${job.result.images_preserved ? ' Saved images were preserved.' : ''}`;
    }
    return 'Operation completed successfully. No detailed log was recorded.';
  }
  return job.status === 'failed' ? 'Operation failed. No detailed log was recorded.' : 'Operation interrupted. No detailed log was recorded.';
}
// Docker's per-layer counts, summarized by the manager while a job downloads an image.
export function downloadProgress(progress) {
  if (!progress) return null;
  const { current = 0, total = null, layers = 0, layers_done: done = 0 } = progress;
  if (!layers) return { text: 'Preparing download…' };
  const counted = `${done} of ${layers} layers`;
  if (!total) return { text: `${total === 0 ? 'Checking layers' : `${fileSize(current)} downloaded`} · ${counted}` };
  if (current >= total && done < layers) return { value: 1, max: 1, percent: 100, text: `Extracting · ${counted}` };
  return { value: current, max: total, percent: Math.floor(current * 100 / total), text: `${fileSize(current)} of ${fileSize(total)} · ${counted}` };
}
export function imageChoices(profiles, records, defaultImage, selected) {
  const choices = new Map();
  // A missing profile downloads when it is used; one whose image does not match its labels cannot be used.
  for (const item of profiles) choices.set(item.profile.reference, { value: item.profile.reference, label: item.available || item.error ? item.profile.name : `${item.profile.name} · downloads first`, disabled: !item.available && !!item.error });
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

// The names the settings dialog uses for settings that need a new container.
const restartNames = { resolution: 'resolution', gpu: 'GPU', audio: 'audio', microphone: 'microphone', sudo: 'sudo', gamepad: 'gamepad', shm_mb: 'shared memory', mounts: 'volumes', cpus: 'removing the CPU limit', memory_mb: 'removing the memory limit' };
export function restartNotice(fields, running) {
  const names = fields.map(field => restartNames[field] || field);
  const list = names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names.at(-1)}` : names[0];
  const subject = list.charAt(0).toUpperCase() + list.slice(1);
  const effect = running ? 'the desktop restarts and open applications close' : 'the desktop stays stopped until you start it';
  return `${subject} ${names.length > 1 ? 'need' : 'needs'} a new container: ${effect}. Files in the container are kept.`;
}
