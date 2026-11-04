using System;

namespace Beatd
{
    public sealed class BeatdAnalyzer : IDisposable
    {
        unsafe void* ptr;

        readonly object sync = new object();

        public unsafe BeatdAnalyzer(uint sampleRate)
        {
            if (sampleRate < 100)
                throw new ArgumentOutOfRangeException(nameof(sampleRate));
            var pointer = NativeMethods.beatd_new(sampleRate);
            if (pointer == null)
                throw new InvalidOperationException("Could not create analyzer.");
            ptr = (void*)pointer;
        }

        unsafe void* Pointer
        {
            get
            {
                if (ptr == null)
                    throw new ObjectDisposedException(nameof(BeatdAnalyzer));
                return ptr;
            }
        }

        public unsafe bool Push(ReadOnlySpan<float> samples)
        {
            lock (sync)
            {
                fixed (float* input = samples)
                {
                    var result = NativeMethods.beatd_push(Pointer, input, (nuint)samples.Length);
                    return result;
                }
            }
        }

        public unsafe float Level
        {
            get
            {
                lock (sync)
                {
                    var value = NativeMethods.beatd_level(Pointer);
                    return value;
                }
            }
        }

        public unsafe float Bpm
        {
            get
            {
                lock (sync)
                {
                    var value = NativeMethods.beatd_bpm(Pointer);
                    return value;
                }
            }
        }

        public unsafe bool TryGetSpectrum(Span<float> output, out int bytesWritten)
        {
            lock (sync)
            {
                fixed (float* outputPtr = output)
                {
                    bytesWritten = checked(
                        (int)NativeMethods.beatd_spectrum(Pointer, outputPtr, (nuint)output.Length)
                    );
                }
                return output.Length >= bytesWritten;
            }
        }

        public unsafe float[] GetSpectrum()
        {
            lock (sync)
            {
                var pointer = Pointer;
                var output = new float[
                    checked((int)NativeMethods.beatd_spectrum(pointer, null, 0))
                ];
                fixed (float* outputPtr = output)
                {
                    NativeMethods.beatd_spectrum(pointer, outputPtr, (nuint)output.Length);
                }
                return output;
            }
        }

        public unsafe uint SampleRate
        {
            get
            {
                lock (sync)
                    return NativeMethods.beatd_sample_rate(Pointer);
            }
        }

        public unsafe float BpmRangeMin
        {
            get
            {
                lock (sync)
                    return NativeMethods.beatd_bpm_range_min(Pointer);
            }
            set
            {
                lock (sync)
                {
                    if (!NativeMethods.beatd_set_bpm_range_min(Pointer, value))
                        throw new ArgumentOutOfRangeException(
                            nameof(value),
                            "Use 0 or a value from 25 to 300 BPM."
                        );
                }
            }
        }

        public unsafe float BandLevel(float low, float high)
        {
            lock (sync)
                return NativeMethods.beatd_band_level(Pointer, low, high);
        }

        public unsafe void SetBpmEnabled(bool enabled)
        {
            lock (sync)
                NativeMethods.beatd_set_bpm_enabled(Pointer, enabled);
        }

        public unsafe void SetSpectrumGain(float gain)
        {
            lock (sync)
                NativeMethods.beatd_set_spectrum_gain(Pointer, gain);
        }

        public unsafe void SetSpectrumCompression(float compression)
        {
            lock (sync)
                NativeMethods.beatd_set_spectrum_compression(Pointer, compression);
        }

        public unsafe bool TryGetWaveform(Span<float> output, out int requiredLength)
        {
            lock (sync)
            {
                fixed (float* p = output)
                    requiredLength = checked(
                        (int)NativeMethods.beatd_waveform(Pointer, p, (nuint)output.Length)
                    );
                return output.Length >= requiredLength;
            }
        }

        public unsafe float[] GetWaveform()
        {
            lock (sync)
            {
                var pointer = Pointer;
                var output = new float[
                    checked((int)NativeMethods.beatd_waveform(pointer, null, 0))
                ];
                fixed (float* p = output)
                    NativeMethods.beatd_waveform(pointer, p, (nuint)output.Length);
                return output;
            }
        }

        public unsafe bool TryGetBeatOffsets(float bpm, Span<float> output, out int requiredLength)
        {
            lock (sync)
            {
                fixed (float* p = output)
                    requiredLength = checked(
                        (int)NativeMethods.beatd_beat_offsets(Pointer, bpm, p, (nuint)output.Length)
                    );
                return output.Length >= requiredLength;
            }
        }

        public unsafe float[] GetBeatOffsets(float bpm)
        {
            lock (sync)
            {
                var pointer = Pointer;
                var output = new float[
                    checked((int)NativeMethods.beatd_beat_offsets(pointer, bpm, null, 0))
                ];
                fixed (float* p = output)
                    NativeMethods.beatd_beat_offsets(pointer, bpm, p, (nuint)output.Length);
                return output;
            }
        }

        public unsafe void Reset()
        {
            lock (sync)
            {
                NativeMethods.beatd_reset(Pointer);
            }
        }

        public void Dispose()
        {
            DisposeCore();
            GC.SuppressFinalize(this);
        }

        unsafe void DisposeCore()
        {
            lock (sync)
            {
                if (ptr != null)
                {
                    NativeMethods.beatd_free(ptr);
                    ptr = null;
                }
            }
        }

        ~BeatdAnalyzer()
        {
            DisposeCore();
        }
    }
}
