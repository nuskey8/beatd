using Beatd;

namespace Beatd.Tests;

public class AnalyzerApiTests
{
    [Test]
    public async Task SettingsWaveformAndBeatOffsets()
    {
        using var analyzer = new BeatdAnalyzer(48_000);
        await Assert.That(analyzer.SampleRate).IsEqualTo(48_000u);
        analyzer.BpmRangeMin = 90;
        await Assert.That(analyzer.BpmRangeMin).IsEqualTo(90f);
        analyzer.SetSpectrumGain(2);
        analyzer.SetSpectrumCompression(2);
        analyzer.Push(Enumerable.Repeat(0.5f, 4096).ToArray());
        await Assert.That(analyzer.BandLevel(0, 100) > 0).IsTrue();
        var waveform = analyzer.GetWaveform();
        var shortBuffer = new float[1];
        var fits = analyzer.TryGetWaveform(shortBuffer, out var length);
        await Assert.That(fits).IsFalse();
        await Assert.That(length).IsEqualTo(waveform.Length);
        await Assert.That(shortBuffer[0]).IsEqualTo(waveform[0]);
        var beats = analyzer.GetBeatOffsets(120);
        await Assert.That(beats.Length > 0).IsTrue();
        var output = new float[beats.Length];
        fits = analyzer.TryGetBeatOffsets(120, output, out length);
        await Assert.That(fits).IsTrue();
        await Assert.That(length).IsEqualTo(beats.Length);
        await Assert.That(output.SequenceEqual(beats)).IsTrue();
        await Assert.That(analyzer.GetBeatOffsets(float.NaN).Length).IsEqualTo(0);
        analyzer.SetBpmEnabled(false);
        await Assert.That(analyzer.Bpm).IsEqualTo(0f);
        analyzer.Reset();
        await Assert.That(analyzer.BpmRangeMin).IsEqualTo(90f);
    }
}
