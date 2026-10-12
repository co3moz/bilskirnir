class PcmOutput extends AudioWorkletProcessor {
  constructor() {
    super(); this.buffer = new Float32Array(48000); this.read = 0; this.write = 0; this.count = 0;
    this.port.onmessage = ({ data }) => {
      const samples = new Float32Array(data);
      // Bound latency, drop stale queued sound rather than accumulating seconds of audio.
      if (this.count + samples.length > this.buffer.length) { this.read = this.write; this.count = 0; }
      for (let i = Math.max(0, samples.length - this.buffer.length); i < samples.length; i++) {
        this.buffer[this.write] = Number.isFinite(samples[i]) ? samples[i] : 0;
        this.write = (this.write + 1) % this.buffer.length; this.count++;
      }
    };
  }
  process(_inputs, outputs) {
    const [left, right] = outputs[0];
    for (let i = 0; i < left.length; i++) {
      if (this.count >= 2) {
        left[i] = this.buffer[this.read]; this.read = (this.read + 1) % this.buffer.length;
        right[i] = this.buffer[this.read]; this.read = (this.read + 1) % this.buffer.length; this.count -= 2;
      } else { left[i] = right[i] = 0; }
    }
    return true;
  }
}
class PcmInput extends AudioWorkletProcessor {
  constructor() { super(); this.buffer = new Float32Array(2048); this.offset = 0; }
  process(inputs) {
    const input = inputs[0]?.[0]; if (!input) return true;
    for (const sample of input) {
      this.buffer[this.offset++] = sample;
      if (this.offset === this.buffer.length) {
        const chunk = this.buffer.slice().buffer; this.port.postMessage(chunk, [chunk]); this.offset = 0;
      }
    }
    return true;
  }
}
registerProcessor('pcm-output', PcmOutput);
registerProcessor('pcm-input', PcmInput);

