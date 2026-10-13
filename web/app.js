import RFB from '@novnc/novnc';
import { DesktopMedia } from './media.mjs';
import { icon } from './icons.mjs';
import { FileUploads, fileSize } from './uploads.mjs';
import { escapeHtml as e, memory, date, imageTree, imageChoices, jobLogText } from './helpers.mjs';

const app = document.querySelector('#app');
const modal = document.querySelector('#modal');
const content = document.querySelector('#modal-content');
const state = { tab: 'environments', environments: [], images: [], profiles: [], volumes: [], jobs: [], settings: {}, system: {}, auth: false, desktop: null };
let rfb, remoteClipboard = '', refreshing = false, authEpoch = 0, activeJob = null, jobRefreshing = false;
const media = new DesktopMedia({
  createContext: () => new AudioContext({ sampleRate: 48000 }),
  createNode: (ctx, name, options) => new AudioWorkletNode(ctx, name, options),
  createSocket: url => new WebSocket(url), url: wsUrl,
  getUserMedia: navigator.mediaDevices?.getUserMedia?.bind(navigator.mediaDevices),
  onError: message => toast(message, true),
  onState: status => {
    const output = document.querySelector('[data-audio]'), input = document.querySelector('[data-mic]');
    if (output) output.textContent = status.output ? '♫ Mute' : status.outputPending ? 'Starting audio…' : '♫ Audio';
    if (input) input.textContent = status.input ? '🎙 Disable mic' : status.inputPending ? 'Waiting for permission…' : '🎙 Microphone';
  },
});
const labels = { running: 'Running', exited: 'Stopped', created: 'Starting', missing: 'Missing', unavailable: 'Docker unavailable', restarting: 'Restarting', paused: 'Paused', queued: 'Queued', completed: 'Completed', failed: 'Failed', interrupted: 'Interrupted', ready: 'Ready', not_ready: 'Not built', unhealthy: 'Desktop error' };
const uploads = new FileUploads({ createRequest: () => new XMLHttpRequest(), onChange: updateUploads, onUnauthorized: loginPage });

async function api(path, body, method = body === undefined ? 'GET' : 'POST') {
  const response = await fetch(`/api/${path}`, { method, credentials: 'same-origin', headers: { 'Content-Type': 'application/json', 'X-Wsld-Request': '1' }, ...(body !== undefined ? { body: JSON.stringify(body) } : {}) });
  if (response.status === 401 && path !== 'login') { if (!document.querySelector('#login-form')) loginPage(); throw new Error('Session expired. Sign in again.'); }
  if (!response.ok) {
    const data = await response.json().catch(() => ({}));
    throw new Error(data.error || `Request failed (${response.status})`);
  }
  return response.status === 204 ? null : response.json();
}
function toast(message, error = false) {
  const node = document.createElement('div'); node.className = `toast ${error ? 'error' : ''}`; node.textContent = message;
  document.querySelector('#toasts').append(node); setTimeout(() => node.remove(), 6500);
}
function show(title, html) {
  if (document.querySelector('#upload-panel')) uploads.cancel();
  content.innerHTML = `<header class="dialog-head"><h2>${e(title)}</h2><button class="icon-button" data-close aria-label="Close">${icon('close')}</button></header>${html}`;
  activeJob = null; if (!modal.open) modal.showModal();
}
function badge(value) { return `<span class="badge ${e(value)}">${e(labels[value] || value)}</span>`; }
const jobNames = {create:'Create environment',retry:'Retry setup',start:'Start',stop:'Stop',restart:'Restart',delete:'Delete environment',internet:'Internet',build:'Build image','profile-build':'Build profile',snapshot:'Snapshot',exec:'Run script'};
function shell() {
  app.innerHTML = `<aside><a class="brand" href="/"><img class="brand-mark" src="/mark.svg" alt=""><span>Bilskirnir</span></a>
    <nav>${[['environments','Environments'],['images','Images'],['volumes','Volumes'],['jobs','Jobs']].map(([id,label]) => `<button data-tab="${id}" class="${state.tab === id ? 'active' : ''}"><span>${icon(id)}</span>${label}${id === 'jobs' ? '<small id="job-count"></small>' : ''}</button>`).join('')}</nav>
    ${state.auth ? '<footer><button class="text-button" data-logout>Sign out</button></footer>' : ''}</aside>
    <main><div id="main-content"></div></main>`;
  render();
}
function render() {
  if (state.desktop || !document.querySelector('#main-content')) return;
  for (const button of document.querySelectorAll('[data-tab]')) button.classList.toggle('active', button.dataset.tab === state.tab);
  const pending = state.jobs.filter(j => ['queued','running'].includes(j.status)).length;
  document.querySelector('#job-count').textContent = pending || '';
  const target = document.querySelector('#main-content');
  if (state.tab === 'environments') {
    const running = state.environments.filter(x => x.state === 'running').length;
    target.innerHTML = `<section class="page-head"><h1>Environments</h1><button class="primary" data-new>${icon('plus')}New environment</button></section>
      <section class="metrics"><div><span>TOTAL</span><strong>${state.environments.length}</strong></div><div><span>RUNNING</span><strong class="accent">${running}</strong></div><div><span>CPU</span><strong>${state.system.cpus || '—'}</strong></div><div><span>MEMORY</span><strong>${memory(state.system.memory_bytes)}</strong></div></section>
      ${state.system.connected === false ? `<div class="notice error">${e(state.system.error || 'Cannot connect to Docker.')}</div>` : ''}
      ${state.environments.length ? `<section class="cards">${state.environments.map(card).join('')}</section>` : `<div class="empty"><h2>No environments</h2><button class="primary" data-new>${icon('plus')}New environment</button></div>`}`;
  } else if (state.tab === 'images') {
    target.innerHTML = `<section class="page-head"><h1>Images</h1><div class="actions"><button data-import>Import image</button><button class="primary" data-build>${icon('plus')}Build image</button></div></section>
      <div class="section-line"><h2>Profiles</h2></div>
      <section class="profile-grid">${state.profiles.map(item => `<article class="profile-card"><div class="card-title"><h3>${e(item.profile.name)}</h3>${badge(item.available ? 'ready' : 'not_ready')}</div><code>${e(item.profile.reference)}</code><div class="card-footer">${item.available ? `<button class="primary" data-profile-create="${e(item.profile.reference)}">Create environment ↗</button>` : `<button class="primary" data-profile-build="${e(item.profile.id)}">Build image</button>`}</div>${item.error ? `<p class="inline-error">${e(item.error)}</p>` : ''}</article>`).join('')}</section>
      <div class="section-line"><h2>Snapshots & custom images</h2></div>
      <section class="panel image-tree">${state.images.length ? imageTree(state.images).map(r => `<article style="--depth:${Math.min(r.depth,8)}"><span class="tree-mark">${icon('images')}</span><div><h3>${e(r.name)} ${badge(r.kind)}</h3><code>${e(r.reference)}</code>${r.description ? `<p>${e(r.description)}</p>` : ''}<small>${date(r.created_at)}${r.parent_image_id ? ` · Parent ${e(r.parent_image_id.slice(0,19))}…` : ''}</small></div><button data-clone="${e(r.id)}" ${r.available === false ? 'disabled title="Image no longer available"' : ''}>Create environment ↗</button></article>`).join('') : '<div class="empty"><h2>No saved images</h2></div>'}</section>`;
  } else if (state.tab === 'volumes') {
    target.innerHTML = `<section class="page-head"><h1>Volumes</h1><button class="primary" data-volume>${icon('plus')}New volume</button></section>
      <section class="panel table-wrap"><table><thead><tr><th>NAME</th><th>DRIVER</th><th>SCOPE</th></tr></thead><tbody>${state.volumes.map(v => `<tr><td><span class="accent">${icon('volumes')}</span> <code>${e(v.Name)}</code></td><td>${e(v.Driver)}</td><td>${e(v.Scope)}</td></tr>`).join('') || '<tr><td colspan="3" class="muted">No volumes.</td></tr>'}</tbody></table></section>`;
  } else {
    target.innerHTML = `<section class="page-head"><h1>Jobs</h1></section><section class="panel table-wrap"><table><thead><tr><th>ACTION</th><th>STATUS</th><th>STARTED</th><th></th></tr></thead><tbody>${state.jobs.map(j => `<tr><td>${e(jobNames[j.kind] || j.kind)}</td><td>${badge(j.status)}</td><td>${date(j.created_at)}</td><td><button data-job="${e(j.id)}">View log</button></td></tr>`).join('') || '<tr><td colspan="4" class="muted">No jobs.</td></tr>'}</tbody></table></section>`;
  }
  for (const image of document.querySelectorAll('.preview img')) image.addEventListener('error', () => { image.hidden = true; });
}
function card(item) {
  const env = item.environment, spec = env.spec, running = item.state === 'running', retry = !running && (item.state === 'missing' || !!env.last_error);
  return `<article class="environment-card" data-drop-environment="${env.id}"><div class="preview"><div class="preview-placeholder"><span>${icon('display')}</span><small>${running ? 'Loading preview' : 'Desktop stopped'}</small></div>${running ? `<img alt="${e(spec.name)} desktop preview" src="/api/environments/${env.id}/screenshot?t=${Math.floor(Date.now()/10000)}" loading="lazy">` : ''}<span class="preview-tag">${e(spec.resolution)}</span></div>
    <div class="card-body"><div class="card-title"><h3>${e(spec.name)}</h3>${badge(item.health === 'unhealthy' ? 'unhealthy' : item.health === 'starting' && running ? 'created' : item.state)}</div><p class="desktop-profile">${e(env.desktop?.title || state.profiles.find(p => p.profile.reference === spec.image)?.profile.name || (spec.image === 'wsl-desktop/ubuntu:24.04' ? 'Ubuntu 24.04 · Lightweight XFCE (legacy)' : 'Custom desktop'))}</p><div class="chips"><span>${spec.gpu ? `${icon('gpu')}GPU` : 'CPU only'}</span><span>${spec.cpus ? `${spec.cpus} CPU` : 'Unlimited CPU'}</span><span>${spec.memory_mb ? `${spec.memory_mb} MB` : 'Unlimited RAM'}</span>${spec.audio ? '<span>♫ Audio</span>' : ''}</div>
    <div class="internet-row"><span>Internet</span><button class="toggle ${spec.internet ? 'on' : ''}" data-internet="${env.id}" role="switch" aria-checked="${spec.internet}" aria-label="${e(spec.name)} internet access"><i></i></button></div>
    ${env.last_error ? `<p class="inline-error">${e(env.last_error)}</p>` : ''}<div class="card-footer"><button class="${running ? 'primary' : ''}" ${running ? `data-connect="${env.id}"` : retry ? `data-retry="${env.id}"` : `data-action="start" data-id="${env.id}"`}>${running ? 'Connect ↗' : retry ? '↻ Retry setup' : '▶ Start'}</button><button class="icon-button" data-upload="${env.id}" aria-label="Upload files to ${e(spec.name)}" title="${running ? 'Upload files' : 'Start the environment to upload files'}" ${running ? '' : 'disabled'}>${icon('upload')}</button><button class="icon-button" data-details="${env.id}" aria-label="Environment actions">${icon('more')}</button></div></div></article>`;
}
async function refresh() {
  if (refreshing || !document.querySelector('#main-content')) return;
  refreshing = true;
  const epoch = authEpoch;
  try {
    const paths = ['environments','images','profiles','volumes','jobs','system'];
    const results = await Promise.allSettled(paths.map(p => api(p)));
    if (epoch !== authEpoch) return;
    for (let i = 0; i < paths.length; i++) if (results[i].status === 'fulfilled') state[paths[i]] = results[i].value;
    const errors = results.filter(r => r.status === 'rejected').map(r => r.reason.message);
    if (errors.length) toast([...new Set(errors)].join(' · '), true);
    render();
  } finally { refreshing = false; }
}
function imageOptions(selected = state.settings.base_image) {
  const choices = imageChoices(state.profiles, state.images, state.settings.base_image, selected);
  const active = choices.some(item => item.value === selected && !item.disabled) ? selected : choices.find(item => !item.disabled)?.value;
  return (active ? '' : '<option value="" selected disabled>Build an image first</option>') + choices.map(item => `<option value="${e(item.value)}" ${active === item.value ? 'selected' : ''} ${item.disabled ? 'disabled' : ''}>${e(item.label)}${item.disabled ? ' · not built' : ''}</option>`).join('');
}
function createDialog(selected) {
  show('New environment', `<form id="create-form"><label>Name<input name="name" required maxlength="100" placeholder="Unity workspace"></label><label>Image / snapshot<select name="image" required>${imageOptions(selected)}</select></label>
    <div class="form-grid"><label>Resolution<select name="resolution"><option>1920x1080</option><option>1600x900</option><option>1280x720</option><option>2560x1440</option><option>3840x2160</option></select></label><label>Shared memory (MB)<input name="shm_mb" type="number" min="64" value="512" required></label><label>CPU limit<input name="cpus" type="number" min="0.1" step="0.1" placeholder="Unlimited"></label><label>Memory limit (MB)<input name="memory_mb" type="number" min="256" placeholder="Unlimited"></label></div>
    <div class="checkboxes"><label><input type="checkbox" name="gpu" checked> GPU</label><label><input type="checkbox" name="internet" checked> Internet</label><label><input type="checkbox" name="sudo" checked> Sudo</label><label><input type="checkbox" name="audio" ${state.settings.audio_enabled ? '' : 'disabled'}> Audio</label><label><input type="checkbox" name="microphone" ${state.settings.microphone_enabled ? '' : 'disabled'}> Microphone</label></div>
    <div class="section-line"><h3>Volumes</h3><button type="button" data-add-mount>＋ Mount</button></div><div id="mounts"></div><p class="hint">Mounted volumes are excluded from snapshots.</p><div class="form-error" role="alert"></div><footer class="dialog-footer"><button type="button" data-close>Cancel</button><button class="primary" type="submit">Create</button></footer></form>`);
}
function addMount() {
  if (!state.volumes.length) { toast('Create a volume first.', true); return; }
  const row = document.createElement('div'); row.className = 'mount-row';
  row.innerHTML = `<select aria-label="Volume">${state.volumes.map(v => `<option>${e(v.Name)}</option>`).join('')}</select><input placeholder="/workspace/project" aria-label="Mount path" required><label><input type="checkbox">Read-only</label><button type="button" data-remove-mount aria-label="Remove mount">×</button>`;
  document.querySelector('#mounts').append(row);
}
function details(id) {
  const item = state.environments.find(x => x.environment.id === id), env = item?.environment;
  if (!env) return;
  show(env.spec.name, `<div class="detail-actions">${item.state === 'missing' || env.last_error ? `<button data-retry="${id}">↻ Retry setup</button>` : ''}${['start','stop','restart'].map((action,i) => `<button data-action="${action}" data-id="${id}">${['▶ Start','■ Stop','↻ Restart'][i]}</button>`).join('')}<button data-upload="${id}" ${item.state === 'running' ? '' : 'disabled'}>${icon('upload')}Upload files</button><button data-snapshot="${id}">◇ Snapshot</button><button data-logs="${id}">Logs</button><button data-diagnostics="${id}">Diagnostics</button><button class="danger" data-delete="${id}">Delete environment</button></div>`);
}
function uploadDialog(id, files = []) {
  const item = state.environments.find(item => item.environment.id === id);
  const env = item?.environment || (state.desktop?.id === id ? state.desktop : null);
  if (!env || (item && item.state !== 'running')) { toast('Start the environment before uploading files.', true); return; }
  if (uploads.running) { toast('Wait for the current upload or cancel it first.', true); return; }
  uploads.select([], state.settings.upload_limit_bytes);
  const home = '/home/desktop/Downloads';
  const homeReadOnly = env.spec.mounts.some(mount=>mount.readonly && (home===mount.target || home.startsWith(`${mount.target}/`)));
  const locations = [...(homeReadOnly ? [] : [{target:home,name:'Downloads'}]), ...env.spec.mounts.filter(mount=>!mount.readonly).map(mount=>({target:mount.target,name:`${mount.volume} · ${mount.target}`}))];
  if (!locations.length) { toast('No writable upload destinations are available.', true); return; }
  show('Upload files', `<section id="upload-panel" data-id="${id}"><p class="upload-environment">${e(env.spec.name)}</p><div class="form-grid"><label>Destination<select id="upload-destination">${locations.map(location=>`<option value="${e(location.target)}">${e(location.name)}</option>`).join('')}</select></label><label>Subfolder<input id="upload-subfolder" placeholder="Optional" autocomplete="off"></label></div><label class="check"><input type="checkbox" id="upload-replace">Replace existing files</label><button class="upload-drop" data-upload-drop data-choose-files>${icon('upload')}<strong>Drop files here or choose files</strong><span>${fileSize(state.settings.upload_limit_bytes)} per file</span></button><input type="file" id="upload-files" multiple hidden><div id="upload-list" class="upload-list"></div><p id="upload-status" class="hint" role="status" aria-live="polite"></p><p class="hint">Volume files are excluded from snapshots. Completed files are kept when you cancel.</p><div class="form-error" role="alert"></div><footer class="dialog-footer"><button data-cancel-upload hidden>Cancel upload</button><button class="primary" data-start-upload disabled>Upload files</button></footer></section>`);
  selectUploadFiles(files);
}
function selectUploadFiles(files) {
  try { uploads.select(files, state.settings.upload_limit_bytes); document.querySelector('#upload-panel .form-error').textContent = ''; }
  catch (error) { document.querySelector('#upload-panel .form-error').textContent = error.message; }
}
function updateUploads() {
  const panel = document.querySelector('#upload-panel'); if (!panel) return;
  panel.classList.toggle('has-files', uploads.items.length > 0);
  const statuses = {queued:'Ready',uploading:'Uploading',saving:'Saving',completed:'Uploaded',failed:'Failed',cancelled:'Cancelled'};
  panel.querySelector('#upload-list').innerHTML = uploads.items.map(item=>`<article class="upload-item"><div><strong>${e(item.name)}</strong><small>${fileSize(item.size)} · ${statuses[item.status]}</small></div><progress value="${item.loaded}" max="${Math.max(item.size,1)}" aria-label="${e(item.name)} upload progress"></progress>${item.error ? `<p class="form-error">${e(item.error)}</p>` : ''}${item.path ? `<code>${e(item.path)}</code>` : ''}</article>`).join('');
  const completed = uploads.items.filter(item=>item.status==='completed').length;
  const status = uploads.items.length ? `${completed} of ${uploads.items.length} files uploaded` : '';
  if (panel.querySelector('#upload-status').textContent !== status) panel.querySelector('#upload-status').textContent = status;
  for (const control of panel.querySelectorAll('select,input,[data-choose-files]')) control.disabled = uploads.running;
  const start = panel.querySelector('[data-start-upload]');
  start.disabled = uploads.running || !uploads.items.some(item=>item.status!=='completed');
  start.textContent = uploads.running ? 'Uploading…' : uploads.items.some(item=>['failed','cancelled'].includes(item.status)) ? 'Retry remaining files' : 'Upload files';
  panel.querySelector('[data-cancel-upload]').hidden = !uploads.running;
}
function buildDialog() {
  show('Build image', `<form id="build-form"><label>Name<input name="name" required maxlength="100" placeholder="Unity tools"></label><label>Base image<select name="base_image" required>${imageOptions()}</select></label><label>Setup script<textarea name="script" class="code-input" rows="12" required spellcheck="false">#!/usr/bin/env bash
set -euo pipefail
apt-get update
apt-get install -y git build-essential
rm -rf /var/lib/apt/lists/*</textarea></label><label class="check"><input name="internet" type="checkbox" checked> Internet during build</label><p class="hint">Runs as root during the build.</p><div class="form-error" role="alert"></div><footer class="dialog-footer"><button type="button" data-close>Cancel</button><button class="primary">Build image</button></footer></form>`);
}
async function submitJob(path, body, method) {
  const result = await api(path, body, method); modal.close(); toast('Job queued.');
  await refresh(); if (!document.querySelector('#login-form')) await openJob(result.job_id); return result;
}
async function openJob(id) {
  const data = await api(`jobs/${id}`);
  show('Job log', '<div id="job-status"></div><pre id="job-log" class="log-output"></pre><div id="job-result"></div>');
  activeJob = id; updateJob(data);
}
function updateJob(data) {
  if (!modal.open || data.id !== activeJob) return;
  const log = document.querySelector('#job-log'), follow = log.scrollHeight - log.scrollTop - log.clientHeight < 48;
  const text = jobLogText(data);
  if (log.textContent !== text) { log.textContent = text; if (follow) log.scrollTop = log.scrollHeight; }
  document.querySelector('#job-status').innerHTML = `<p>${badge(data.status)} <span class="hint">${e(jobNames[data.kind] || data.kind)} · ${date(data.created_at)}</span></p>`;
  const result = data.result;
  document.querySelector('#job-result').innerHTML = result ? `${result.reference ? `<p class="hint">${e(result.reference)}</p>` : ''}${result.image_id && result.reference ? `<button data-profile-create="${e(result.image_id)}">Create environment ↗</button>` : ''}${result.environment_id ? '<button data-job-environments>View environments ↗</button>' : ''}` : '';
}
async function pollJob() {
  if (!activeJob || !modal.open || jobRefreshing) return;
  const id = activeJob; jobRefreshing = true;
  try { const data = await api(`jobs/${id}`); if (id === activeJob) updateJob(data); }
  catch (error) { if (id === activeJob) { activeJob = null; toast(error.message, true); } }
  finally { jobRefreshing = false; }
}
function loginPage() {
  authEpoch++; activeJob = null; modal.close(); closeDesktop();
  app.innerHTML = `<section class="login-page"><div class="login-card"><a class="brand" href="/"><img class="brand-mark" src="/mark.svg" alt=""><span>Bilskirnir</span></a><h1>Sign in</h1><form id="login-form"><label>Username<input name="username" autocomplete="username" required></label><label>Password<input name="password" type="password" autocomplete="current-password" required></label><div class="form-error" role="alert"></div><button class="primary">Sign in</button></form></div></section>`;
}
async function initialize() {
  const session = await api('session'); state.auth = session.auth_enabled;
  if (!session.authenticated) return loginPage();
  state.settings = await api('settings'); shell(); await refresh();
}
function wsUrl(path) { return `${location.protocol === 'https:' ? 'wss' : 'ws'}://${location.host}/api/${path}`; }
function closeDesktop() { const old = rfb; rfb = null; old?.disconnect(); media.close(); state.desktop = null; remoteClipboard = ''; }
function connect(id) {
  const env = state.environments.find(x => x.environment.id === id)?.environment; if (!env) return;
  closeDesktop(); state.desktop = env; media.select(env.id);
  app.innerHTML = `<section class="desktop-view" data-drop-environment="${id}"><header class="desktop-toolbar"><button data-back>← Environments</button><h2>${e(env.spec.name)}</h2><span id="vnc-status" class="muted">Connecting…</span><div class="spacer"></div><button data-upload="${id}">${icon('upload')}Upload files</button><button data-clipboard>Clipboard</button>${env.spec.audio ? '<button data-audio>♫ Audio</button>' : ''}${env.spec.microphone ? '<button data-mic>🎙 Microphone</button>' : ''}<button data-reconnect>↻</button><button data-fullscreen>Fullscreen</button></header><div id="screen"></div></section>`;
  connectRfb();
}
function connectRfb() {
  const old = rfb; rfb = null; old?.disconnect(); media.close(); media.select(state.desktop?.id);
  const id = state.desktop?.id; if (!id) return;
  const status = document.querySelector('#vnc-status'); status.textContent = 'Connecting…';
  try {
    const connection = new RFB(document.querySelector('#screen'), wsUrl(`environments/${id}/vnc`));
    rfb = connection;
    rfb.scaleViewport = true; rfb.resizeSession = true; rfb.showDotCursor = true; rfb.focusOnClick = true;
    rfb.addEventListener('connect', () => { if (rfb !== connection) return; status.textContent = 'Connected'; status.className = 'accent'; });
    rfb.addEventListener('disconnect', event => { if (rfb !== connection) return; media.close(); status.textContent = event.detail.clean ? 'Disconnected' : 'Connection failed'; status.className = 'muted'; });
    rfb.addEventListener('clipboard', event => { if (rfb !== connection) return; remoteClipboard = event.detail.text; if (document.querySelector('#clipboard-text')) document.querySelector('#clipboard-text').value = event.detail.text; });
  } catch (error) { status.textContent = error.message; }
}
document.addEventListener('click', async event => {
  const button = event.target.closest('button'); if (!button) return;
  try {
    if (button.hasAttribute('data-close')) modal.close();
    else if (button.dataset.tab) { state.tab = button.dataset.tab; render(); }
    else if (button.hasAttribute('data-new')) createDialog();
    else if (button.dataset.profileCreate) createDialog(button.dataset.profileCreate);
    else if (button.dataset.profileBuild) { button.disabled = true; await submitJob(`profiles/${button.dataset.profileBuild}/build`, {}); }
    else if (button.dataset.clone) { const img = state.images.find(r => r.id === button.dataset.clone); createDialog(img.image_id); }
    else if (button.hasAttribute('data-add-mount')) addMount();
    else if (button.hasAttribute('data-remove-mount')) button.closest('.mount-row').remove();
    else if (button.dataset.details) details(button.dataset.details);
    else if (button.dataset.connect) connect(button.dataset.connect);
    else if (button.dataset.upload) uploadDialog(button.dataset.upload);
    else if (button.hasAttribute('data-choose-files')) document.querySelector('#upload-files').click();
    else if (button.hasAttribute('data-cancel-upload')) uploads.cancel();
    else if (button.hasAttribute('data-start-upload')) {
      const panel = document.querySelector('#upload-panel'), base = panel.querySelector('#upload-destination').value, folder = panel.querySelector('#upload-subfolder').value.trim();
      const error = panel.querySelector('.form-error'); error.textContent = '';
      if (folder && (folder.startsWith('/') || folder.includes('\\') || folder.split('/').some(part=>!part || part==='.' || part==='..'))) { error.textContent = 'Enter a relative subfolder without . or .. segments.'; return; }
      await uploads.start({environmentId:panel.dataset.id,destination:folder ? `${base}/${folder}` : base,replace:panel.querySelector('#upload-replace').checked});
    }
    else if (button.hasAttribute('data-back')) { closeDesktop(); shell(); await refresh(); }
    else if (button.hasAttribute('data-reconnect')) connectRfb();
    else if (button.hasAttribute('data-fullscreen')) { if (document.fullscreenElement) await document.exitFullscreen(); else await document.querySelector('.desktop-view').requestFullscreen(); }
    else if (button.dataset.retry) { button.disabled = true; await submitJob(`environments/${button.dataset.retry}/retry`, {}); }
    else if (button.hasAttribute('data-job-environments')) { modal.close(); state.tab = 'environments'; render(); await refresh(); }
    else if (button.dataset.action) { button.disabled = true; await submitJob(`environments/${button.dataset.id}/action`, { action: button.dataset.action }); }
    else if (button.dataset.internet) { button.disabled = true; const env = state.environments.find(r => r.environment.id === button.dataset.internet).environment; await submitJob(`environments/${env.id}/internet`, { enabled: !env.spec.internet }); }
    else if (button.dataset.snapshot) { modal.close(); show('Create snapshot', `<form id="snapshot-form" data-id="${button.dataset.snapshot}"><label>Name<input name="name" required maxlength="100"></label><label>Description<textarea name="description" rows="3" maxlength="4096"></textarea></label><p class="hint">Briefly pauses the container. Volumes and running application state are excluded.</p><div class="form-error" role="alert"></div><footer class="dialog-footer"><button type="button" data-close>Cancel</button><button class="primary">Create snapshot</button></footer></form>`); }
    else if (button.dataset.delete) { const id = button.dataset.delete; modal.close(); show('Delete environment', `<p>Delete this environment and its unsaved changes? Volumes and snapshots are kept.</p><div class="form-error"></div><footer class="dialog-footer"><button data-close>Cancel</button><button class="danger" data-confirm-delete="${id}">Delete</button></footer>`); }
    else if (button.dataset.confirmDelete) { button.disabled = true; await submitJob(`environments/${button.dataset.confirmDelete}`, undefined, 'DELETE'); }
    else if (button.dataset.job) await openJob(button.dataset.job);
    else if (button.dataset.logs || button.dataset.diagnostics) {
      const path = button.dataset.job ? `jobs/${button.dataset.job}` : `environments/${button.dataset.logs || button.dataset.diagnostics}/${button.dataset.logs ? 'logs' : 'diagnostics'}`;
      const data = await api(path); modal.close(); show('Log', `<pre class="log-output">${e(data.log ?? data.output ?? '')}</pre>${data.status ? `<p>${badge(data.status)}</p>` : ''}`);
    }
    else if (button.hasAttribute('data-build')) buildDialog();
    else if (button.hasAttribute('data-import')) show('Import image', '<form id="import-form"><label>Name<input name="name" required></label><label>Image reference<input name="reference" required placeholder="bilskirnir/ubuntu-xfce:24.04"></label><div class="form-error"></div><footer class="dialog-footer"><button class="primary">Import</button></footer></form>');
    else if (button.hasAttribute('data-volume')) show('New volume', '<form id="volume-form"><label>Name<input name="name" required pattern="[a-zA-Z0-9][a-zA-Z0-9_.-]+" placeholder="unity-projects"></label><div class="form-error"></div><footer class="dialog-footer"><button class="primary">Create</button></footer></form>');
    else if (button.hasAttribute('data-logout')) { await api('logout', {}); closeDesktop(); loginPage(); }
    else if (button.hasAttribute('data-clipboard')) { show('Clipboard', '<label>Text<textarea id="clipboard-text" rows="8" placeholder="Paste text or read the desktop clipboard."></textarea></label><footer class="dialog-footer"><button class="primary" data-paste>Send to desktop</button></footer>'); document.querySelector('#clipboard-text').value = remoteClipboard; }
    else if (button.hasAttribute('data-paste')) { rfb?.clipboardPasteFrom(document.querySelector('#clipboard-text').value); modal.close(); }
    else if (button.hasAttribute('data-audio')) await media.toggleOutput();
    else if (button.hasAttribute('data-mic')) await media.toggleMic();
  } catch (error) { toast(error.message, true); button.disabled = false; }
});
document.addEventListener('change', event => { if (event.target.id === 'upload-files') { selectUploadFiles(event.target.files); event.target.value = ''; return; } const form = event.target.closest('#create-form'); if (!form) return; if (event.target.name === 'microphone' && event.target.checked) form.elements.audio.checked = true; if (event.target.name === 'audio' && !event.target.checked) form.elements.microphone.checked = false; });
document.addEventListener('dragover', event => {
  if (!event.dataTransfer?.types.includes('Files')) return;
  event.preventDefault();
  const target = event.target.closest('[data-upload-drop],[data-drop-environment]');
  event.dataTransfer.dropEffect = target ? 'copy' : 'none';
  target?.classList.add('drag-over');
});
document.addEventListener('dragleave', event => { const target = event.target.closest('[data-upload-drop],[data-drop-environment]'); if (target && !target.contains(event.relatedTarget)) target.classList.remove('drag-over'); });
document.addEventListener('drop', event => {
  if (!event.dataTransfer?.types.includes('Files')) return;
  event.preventDefault();
  document.querySelectorAll('.drag-over').forEach(element=>element.classList.remove('drag-over'));
  if ([...event.dataTransfer.items].some(item=>item.webkitGetAsEntry?.()?.isDirectory)) { toast('Choose files instead of a folder.', true); return; }
  const files = [...event.dataTransfer.files];
  if (!files.length) { toast('Choose files instead of a folder.', true); return; }
  if (event.target.closest('[data-upload-drop]')) selectUploadFiles(files);
  else { const target = event.target.closest('[data-drop-environment]'); if (target) uploadDialog(target.dataset.dropEnvironment, files); else toast('Drop files on an environment card or its desktop.', true); }
});

document.addEventListener('submit', async event => {
  event.preventDefault(); const form = event.target, data = Object.fromEntries(new FormData(form));
  const button = form.querySelector('button[type="submit"],button.primary'); button.disabled = true;
  form.querySelector('.form-error').textContent = '';
  try {
    if (form.id === 'login-form') { await api('login', data); await initialize(); }
    else if (form.id === 'create-form') {
      for (const key of ['gpu','internet','sudo','audio','microphone']) data[key] = form.elements[key].checked;
      for (const key of ['cpus','memory_mb','shm_mb']) data[key] = data[key] ? Number(data[key]) : null;
      data.mounts = [...form.querySelectorAll('.mount-row')].map(row => ({ volume: row.querySelector('select').value, target: row.querySelector('input:not([type])').value, readonly: row.querySelector('[type=checkbox]').checked }));
      await submitJob('environments', data);
    } else if (form.id === 'snapshot-form') await submitJob(`environments/${form.dataset.id}/snapshot`, data);
    else if (form.id === 'build-form') { data.internet = form.elements.internet.checked; await submitJob('images/build', data); }
    else if (form.id === 'volume-form') { await api('volumes', data); modal.close(); await refresh(); }
    else if (form.id === 'import-form') { await api('images', data); modal.close(); await refresh(); }
  } catch (error) { form.querySelector('.form-error').textContent = error.message; } finally { button.disabled = false; }
});
setInterval(() => { if (!document.hidden && !state.desktop) refresh(); }, 8000);
setInterval(() => { if (!document.hidden) pollJob(); }, 2000);
modal.addEventListener('close', () => { activeJob = null; uploads.cancel(); });
window.addEventListener('beforeunload', () => { uploads.cancel(); closeDesktop(); });
initialize().catch(error => { app.innerHTML = `<div class="empty"><h1>Connection failed</h1><p>${e(error.message)}</p></div>`; });

