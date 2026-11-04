using Beatd;
using UnityEngine;

[RequireComponent(typeof(AudioSource))]
public class Sandbox : MonoBehaviour
{
    BeatdAnalyzer analyzer;
    float[] monoSamples;
    float nextLogTime;

    void OnEnable()
    {
        analyzer = new BeatdAnalyzer((uint)AudioSettings.outputSampleRate);
        analyzer.BpmRangeMin = 100f;
        nextLogTime = 0f;
    }

    void OnAudioFilterRead(float[] data, int channels)
    {
        if (channels <= 0)
            return;

        if (analyzer == null)
            return;

        int frames = data.Length / channels;
        if (monoSamples == null || monoSamples.Length != frames)
            monoSamples = new float[frames];

        for (int frame = 0; frame < frames; frame++)
        {
            float sample = 0f;
            for (int channel = 0; channel < channels; channel++)
                sample += data[frame * channels + channel];
            monoSamples[frame] = sample / channels;
        }

        analyzer.Push(monoSamples);
    }

    void Update()
    {
        if (Time.unscaledTime < nextLogTime)
            return;
        nextLogTime = Time.unscaledTime + 1f;

        var bpm = analyzer == null ? 0f : analyzer.Bpm;
        if (bpm > 0f)
            Debug.Log($"BPM: {bpm:F1}", this);
    }

    void OnDisable()
    {
        analyzer?.Dispose();
        analyzer = null;
    }
}
