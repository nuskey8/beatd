using Beatd;

namespace Beatd.Tests;

public class SpectrumTests
{
    [Test]
    public async Task SpectrumBuffersReportRequiredLength()
    {
        using var analyzer = new BeatdAnalyzer(48_000);
        var empty = analyzer.TryGetSpectrum(Span<float>.Empty, out var initialLength);
        await Assert.That(empty).IsTrue();
        await Assert.That(initialLength).IsEqualTo(0);
        await Assert.That(analyzer.GetSpectrum().Length).IsEqualTo(0);

        analyzer.Push(Enumerable.Repeat(0.5f, 2048).ToArray());
        var small = new float[1];
        var fits = analyzer.TryGetSpectrum(small, out var required);
        await Assert.That(fits).IsFalse();
        await Assert.That(required).IsEqualTo(1024);
        var expected = analyzer.GetSpectrum();
        await Assert.That(small[0]).IsEqualTo(expected[0]);

        var exact = new float[required];
        fits = analyzer.TryGetSpectrum(exact, out required);
        await Assert.That(fits).IsTrue();
        await Assert.That(exact.SequenceEqual(expected)).IsTrue();
        var larger = new float[required + 1];
        larger[^1] = -1;
        fits = analyzer.TryGetSpectrum(larger, out required);
        await Assert.That(fits).IsTrue();
        await Assert.That(required).IsEqualTo(expected.Length);
        await Assert.That(larger.Take(required).SequenceEqual(expected)).IsTrue();
        await Assert.That(larger[^1]).IsEqualTo(-1f);
        analyzer.Reset();
        await Assert.That(analyzer.GetSpectrum().Length).IsEqualTo(0);
    }
}
