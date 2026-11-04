#ifndef BEATD_H
#define BEATD_H

#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

typedef struct beatd_analyzer beatd_analyzer;

beatd_analyzer *beatd_new(uint32_t rate);
void beatd_free(beatd_analyzer *analyzer);

bool beatd_push(beatd_analyzer *analyzer, const float *samples, size_t len);

float beatd_level(const beatd_analyzer *analyzer);
float beatd_bpm(const beatd_analyzer *analyzer);
size_t beatd_spectrum(const beatd_analyzer *analyzer, float *out,
                      size_t capacity);
uint32_t beatd_sample_rate(const beatd_analyzer *analyzer);
float beatd_band_level(const beatd_analyzer *analyzer, float low, float high);
float beatd_bpm_range_min(const beatd_analyzer *analyzer);
bool beatd_set_bpm_range_min(beatd_analyzer *analyzer, float min);
void beatd_set_bpm_enabled(beatd_analyzer *analyzer, bool enabled);
void beatd_set_spectrum_gain(beatd_analyzer *analyzer, float gain);
void beatd_set_spectrum_compression(beatd_analyzer *analyzer, float compression);
size_t beatd_waveform(const beatd_analyzer *analyzer, float *out, size_t capacity);
size_t beatd_beat_offsets(const beatd_analyzer *analyzer, float bpm, float *out, size_t capacity);
void beatd_reset(beatd_analyzer *analyzer);

#ifdef __cplusplus
}
#endif
#endif
