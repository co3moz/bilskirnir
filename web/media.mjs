// Every asynchronous callback belongs to one desktop and one channel generation.
export class DesktopMedia {
  constructor(deps) { this.deps = deps; this.session = null; }
  select(id) { this.close(); if (id) this.session = { id, closed: false, outputEpoch: 0, inputEpoch: 0 }; this.notify(); }
  notify() {
    const s = this.session;
    this.deps.onState?.({ output: !!s?.output?.open, input: !!s?.input?.open, outputPending: !!s?.outputPending, inputPending: !!s?.inputPending });
  }
  valid(s, channel, epoch) { return this.session === s && !s.closed && s[`${channel}Epoch`] === epoch; }
  close() {
    const s = this.session; this.session = null;
    if (!s) return;
    s.closed = true; this.dispose(s.output); this.dispose(s.input);
    s.ctx?.close().catch(() => {}); this.notify();
  }
  dispose(channel) {
    if (!channel) return;
    if (channel.socket) { channel.socket.onopen = channel.socket.onclose = channel.socket.onerror = channel.socket.onmessage = null; channel.socket.close(); }
    channel.stream?.getTracks().forEach(t => t.stop());
    for (const node of channel.nodes) { if (node.port) node.port.onmessage = null; node.disconnect(); }
  }
  idle(s) {
    if (!s.output && !s.input && !s.outputPending && !s.inputPending) {
      s.ctx?.close().catch(() => {}); s.ctx = s.contextPromise = null;
    }
  }
  stop(channel) {
    const s = this.session; if (!s) return;
    s[`${channel}Epoch`]++; s[`${channel}Pending`] = false;
    this.dispose(s[channel]); s[channel] = null; this.idle(s); this.notify();
  }
  async context(s) {
    if (!s.contextPromise) {
      const ctx = this.deps.createContext(); s.ctx = ctx;
      s.contextPromise = (async () => {
        if (ctx.sampleRate !== 48000) throw new Error('This browser does not support 48 kHz audio.');
        await ctx.audioWorklet.addModule('/audio-worklet.js');
        if (s.closed || s.ctx !== ctx) return null;
        await ctx.resume(); return ctx;
      })();
      s.contextPromise.catch(() => { if (s.ctx === ctx) { ctx.close().catch(() => {}); s.ctx = s.contextPromise = null; } });
    }
    return s.contextPromise;
  }
  attach(s, channel, resource, epoch) {
    s[channel] = resource; s[`${channel}Pending`] = false;
    const current = () => this.valid(s, channel, epoch) && s[channel] === resource;
    resource.socket.onopen = () => { if (current()) { resource.open = true; this.notify(); } };
    resource.socket.onerror = () => { if (current()) this.deps.onError?.(`${channel === 'output' ? 'Audio' : 'Microphone'} connection failed. Check the environment logs.`); };
    resource.socket.onclose = () => { if (current()) this.stop(channel); };
    this.notify(); return current;
  }
  async toggleOutput() {
    const s = this.session; if (!s) return;
    if (s.output || s.outputPending) { this.stop('output'); return; }
    const epoch = ++s.outputEpoch; s.outputPending = true; this.notify();
    let resource;
    try {
      const ctx = await this.context(s); if (!ctx || !this.valid(s, 'output', epoch)) return;
      const node = this.deps.createNode(ctx, 'pcm-output', { outputChannelCount: [2] });
      resource = { nodes: [node], socket: null }; node.connect(ctx.destination);
      resource.socket = this.deps.createSocket(this.deps.url(`environments/${s.id}/audio/output`));
      resource.socket.binaryType = 'arraybuffer';
      const current = this.attach(s, 'output', resource, epoch);
      let remainder = new Uint8Array();
      resource.socket.onmessage = event => {
        if (!current()) return;
        const incoming = new Uint8Array(event.data), merged = new Uint8Array(remainder.length + incoming.length);
        merged.set(remainder); merged.set(incoming, remainder.length);
        const size = merged.length - merged.length % 8; remainder = merged.slice(size);
        if (size) { const data = merged.slice(0, size).buffer; node.port.postMessage(data, [data]); }
      };
    } catch (error) { if (this.valid(s, 'output', epoch)) { if (resource && !s.output) this.dispose(resource); this.stop('output'); throw error; } }
    finally { if (this.valid(s, 'output', epoch)) { s.outputPending = false; this.idle(s); this.notify(); } }
  }
  async toggleMic() {
    const s = this.session; if (!s) return;
    if (s.input || s.inputPending) { this.stop('input'); return; }
    const epoch = ++s.inputEpoch; s.inputPending = true; this.notify();
    let stream, resource;
    try {
      if (!this.deps.getUserMedia) throw new Error('Microphone access requires HTTPS or localhost.');
      const ctx = await this.context(s); if (!ctx || !this.valid(s, 'input', epoch)) return;
      stream = await this.deps.getUserMedia({ audio: { channelCount: 1, echoCancellation: true }, video: false });
      if (!this.valid(s, 'input', epoch) || s.ctx !== ctx) { stream.getTracks().forEach(t => t.stop()); return; }
      resource = { stream, nodes: [], socket: null };
      const source = ctx.createMediaStreamSource(stream); resource.nodes.push(source);
      const node = this.deps.createNode(ctx, 'pcm-input'); resource.nodes.push(node);
      const mute = ctx.createGain(); resource.nodes.push(mute); mute.gain.value = 0;
      source.connect(node); node.connect(mute); mute.connect(ctx.destination);
      const socket = this.deps.createSocket(this.deps.url(`environments/${s.id}/audio/input`));
      resource.socket = socket; socket.binaryType = 'arraybuffer';
      const current = this.attach(s, 'input', resource, epoch);
      node.port.onmessage = event => { if (current() && socket.readyState === 1 && socket.bufferedAmount < 192000) socket.send(event.data); };
    } catch (error) {
      if (resource && !s.input) this.dispose(resource); else if (!resource) stream?.getTracks().forEach(t => t.stop());
      if (this.valid(s, 'input', epoch)) { this.stop('input'); throw error; }
    } finally { if (this.valid(s, 'input', epoch)) { s.inputPending = false; this.idle(s); this.notify(); } }
  }
}
