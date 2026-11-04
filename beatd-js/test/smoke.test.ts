import { expect, test } from 'bun:test';
import { createAnalyzer } from '../dist/index.js';

test('analyzes samples and resets', () => {
  const analyzer = createAnalyzer({ sampleRate: 48000 });
  expect(analyzer.push(new Float32Array(2048).fill(0.5))).toBe(true);
  expect(analyzer.level()).toBe(0.5);
  expect(analyzer.spectrum()).toHaveLength(1024);
  analyzer.reset();
  expect(analyzer.spectrum()).toHaveLength(0);
  analyzer.free();
});

test('disposes with using', () => {
  let disposed = false;
  const analyzer = createAnalyzer({ sampleRate: 48000 });
  const free = analyzer.free.bind(analyzer);
  analyzer.free = () => {
    disposed = true;
    free();
  };
  {
    using disposable = analyzer;
    expect(disposable.sampleRate()).toBe(48000);
  }
  expect(disposed).toBe(true);
});

test('settings, waveform and beat offsets', () => {
  const analyzer = createAnalyzer({ sampleRate: 48000 });
  expect(analyzer.sampleRate()).toBe(48000);
  analyzer.setBpmRangeMin(90);
  expect(analyzer.bpmRangeMin()).toBe(90);
  analyzer.setSpectrumGain(2);
  analyzer.setSpectrumCompression(2);
  analyzer.push(new Float32Array(4096).fill(0.5));
  expect(analyzer.bandLevel(0, 100)).toBeGreaterThan(0);
  expect(analyzer.waveform().length).toBeGreaterThan(0);
  expect(analyzer.beatOffsets(120).length).toBeGreaterThan(0);
  expect(analyzer.beatOffsets(NaN)).toHaveLength(0);
  analyzer.setBpmEnabled(false);
  analyzer.reset();
  expect(analyzer.bpmRangeMin()).toBe(90);
  analyzer.free();
});
