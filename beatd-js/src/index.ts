import init, { Analyzer as WasmAnalyzer } from '../dist/wasm/beatd.js';

const wasmUrl = new URL('./wasm/beatd_bg.wasm', import.meta.url);
if (wasmUrl.protocol === 'file:') {
  const { readFile } = await import('node:fs/promises');
  await init({ module_or_path: await readFile(wasmUrl) });
} else {
  await init({ module_or_path: wasmUrl });
}

export interface AnalyzerOptions {
  sampleRate: number;
}

export class Analyzer {
  private readonly inner: WasmAnalyzer;

  constructor(options: AnalyzerOptions) {
    this.inner = new WasmAnalyzer(options.sampleRate);
  }

  push(samples: Float32Array): boolean { return this.inner.push(samples); }
  sampleRate(): number { return this.inner.sample_rate(); }
  level(): number { return this.inner.level(); }
  bpm(): number { return this.inner.bpm(); }
  spectrum(): Float32Array { return this.inner.spectrum(); }
  waveform(): Float32Array { return this.inner.waveform(); }
  beatOffsets(bpm: number): Float32Array { return this.inner.beat_offsets(bpm); }
  bandLevel(low: number, high: number): number { return this.inner.band_level(low, high); }
  bpmRangeMin(): number { return this.inner.bpm_range_min(); }
  setBpmRangeMin(min: number): void { this.inner.set_bpm_range_min(min); }
  setBpmEnabled(enabled: boolean): void { this.inner.set_bpm_enabled(enabled); }
  setSpectrumGain(gain: number): void { this.inner.set_spectrum_gain(gain); }
  setSpectrumCompression(compression: number): void { this.inner.set_spectrum_compression(compression); }
  reset(): void { this.inner.reset(); }
  dispose(): void { this.inner.free(); }
  [Symbol.dispose](): void { this.dispose(); }
}

export function createAnalyzer(options: AnalyzerOptions): Analyzer {
  return new Analyzer(options);
}
