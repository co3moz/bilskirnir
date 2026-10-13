export const fileSize = bytes => bytes < 1024 ? `${bytes} B` : bytes < 1024 ** 2 ? `${(bytes / 1024).toFixed(1)} KiB` : bytes < 1024 ** 3 ? `${(bytes / 1024 ** 2).toFixed(1)} MiB` : `${(bytes / 1024 ** 3).toFixed(1)} GiB`;

export class FileUploads {
  constructor({ createRequest, onChange = () => {}, onUnauthorized = () => {} }) {
    Object.assign(this, { createRequest, onChange, onUnauthorized, items: [], running: false, cancelled: false, request: null });
  }
  select(files, limit) {
    if (this.running) throw new Error('Wait for the current upload or cancel it first.');
    if (files.length > 100) throw new Error('Select up to 100 files at a time.');
    if ([...files].some(file => file.size > limit)) throw new Error(`Files must be ${fileSize(limit)} or smaller.`);
    this.items = [...files].map(file => ({ file, name: file.name, size: file.size, loaded: 0, status: 'queued', error: '', path: '' }));
    this.cancelled = false;
    this.onChange();
  }
  cancel() {
    if (!this.running) return;
    this.cancelled = true;
    this.request?.abort();
    for (const item of this.items) if (item.status === 'queued') item.status = 'cancelled';
    this.onChange();
  }
  async start(options) {
    if (this.running) return;
    this.running = true; this.cancelled = false; this.onChange();
    try {
      for (const item of this.items.filter(item => item.status !== 'completed')) {
        if (this.cancelled) break;
        item.status = 'uploading'; item.error = ''; item.loaded = 0; this.onChange();
        try {
          const result = await this.send(item, options);
          item.status = 'completed'; item.loaded = item.size; item.path = result.path;
        } catch (error) {
          item.status = this.cancelled ? 'cancelled' : 'failed'; item.error = error.message;
        }
        this.onChange();
      }
    } finally { this.request = null; this.running = false; this.onChange(); }
  }
  send(item, { environmentId, destination, replace }) {
    return new Promise((resolve, reject) => {
      const request = this.createRequest(); this.request = request;
      const query = new URLSearchParams({ destination, name: item.name, size: String(item.size), replace: String(replace) });
      request.open('POST', `/api/environments/${encodeURIComponent(environmentId)}/files?${query}`);
      request.setRequestHeader('Content-Type', 'application/octet-stream');
      request.setRequestHeader('X-Wsld-Request', '1');
      request.withCredentials = true; request.timeout = 3_600_000;
      request.upload.onprogress = event => { item.loaded = Math.min(event.loaded, item.size); this.onChange(); };
      request.upload.onload = () => { item.status = 'saving'; this.onChange(); };
      request.onload = () => {
        let result;
        try { result = JSON.parse(request.responseText); } catch { result = {}; }
        if (request.status >= 200 && request.status < 300) resolve(result);
        else {
          if (request.status === 401) { this.cancelled = true; this.onUnauthorized(); }
          reject(new Error(result.error || `Upload failed (${request.status}).`));
        }
      };
      request.onerror = () => reject(new Error('Connection lost. Retry the file.'));
      request.ontimeout = () => reject(new Error('Upload timed out.'));
      request.onabort = () => reject(new Error('Upload cancelled.'));
      request.send(item.file);
    });
  }
}
