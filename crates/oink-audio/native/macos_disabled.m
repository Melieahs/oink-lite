// Fallback for macOS SDKs without Core Audio process-tap headers (< 14.2).
//
// Keyboard and mouse sharing never touch this file. Every entry point reports
// "unavailable" so the agent can build and run on macOS 10.15; the UI shows
// audio as unsupported instead of failing to launch.
//
// The real implementation (macos.m) is selected automatically when the SDK
// provides <CoreAudio/AudioHardwareTapping.h> and <CoreAudio/CATapDescription.h>.
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>
#include <mach/mach_time.h>

typedef struct {
    uint64_t callback_ns, host_ns, sequence, gap_ns;
    uint64_t frames, queued_frames, dropped_frames;
} OinkCaptureEvent;

typedef struct {
    double sample_rate;
    double physical_sample_rate;
    double tap_sample_rate;
    uint32_t buffer_frames;
    uint32_t latency_frames;
    uint32_t safety_frames;
    uint32_t physical_device;
    uint32_t physical_buffer_frames;
    uint32_t physical_latency_frames;
    uint32_t physical_safety_frames;
    int32_t buffer_request_status;
    uint32_t physical_original_buffer_frames;
    uint32_t physical_lease_buffer_frames;
    uint32_t physical_lease_changed;
    int32_t physical_buffer_request_status;
    uint32_t capture_mode;
} OinkCaptureInfo;

static void unavailable(char *error, size_t capacity) {
    if (error != NULL && capacity > 0) {
        snprintf(error, capacity, "system audio capture requires macOS 14.2 or later");
    }
}

bool oink_audio_capture_cleanup_failed(void) { return false; }

void *oink_audio_capture_start(bool mute, uint32_t requested_frames, uint32_t capture_mode,
                               const char *selected_uid, bool buffer_before_tap, bool tap_private,
                               bool collect_diagnostics, bool realtime, double *sample_rate,
                               char *error, size_t error_size) {
    (void)mute;
    (void)requested_frames;
    (void)capture_mode;
    (void)selected_uid;
    (void)buffer_before_tap;
    (void)tap_private;
    (void)collect_diagnostics;
    (void)realtime;
    if (sample_rate != NULL) *sample_rate = 0;
    unavailable(error, error_size);
    return NULL;
}

void oink_audio_capture_wake(void *capture) { (void)capture; }

int32_t oink_audio_capture_wait(void *capture) {
    (void)capture;
    return 0;
}

uint32_t oink_audio_capture_device_id(void *capture) {
    (void)capture;
    return 0;
}

size_t oink_audio_capture_read(void *capture, float *output, size_t capacity_frames,
                               uint64_t *host_ns, uint64_t *callback_ns, uint64_t *sequence) {
    (void)capture;
    (void)output;
    (void)capacity_frames;
    if (host_ns != NULL) *host_ns = 0;
    if (callback_ns != NULL) *callback_ns = 0;
    if (sequence != NULL) *sequence = 0;
    return 0;
}

size_t oink_audio_capture_events(void *capture, void *events, size_t capacity, uint64_t *dropped,
                                 uint64_t *trimmed, uint64_t *audio_dropped) {
    (void)capture;
    (void)events;
    (void)capacity;
    if (dropped != NULL) *dropped = 0;
    if (trimmed != NULL) *trimmed = 0;
    if (audio_dropped != NULL) *audio_dropped = 0;
    return 0;
}

void oink_audio_capture_info(void *capture, OinkCaptureInfo *info) {
    (void)capture;
    if (info != NULL) memset(info, 0, sizeof(*info));
}

bool oink_audio_capture_physical_uid(void *capture, char *buffer, size_t capacity) {
    (void)capture;
    if (buffer != NULL && capacity > 0) buffer[0] = '\0';
    return false;
}

int32_t oink_audio_default_output_uid(char *buffer, size_t capacity) {
    if (buffer != NULL && capacity > 0) buffer[0] = '\0';
    return -1;
}

size_t oink_audio_capture_diagnostics(void *capture, char *buffer, size_t capacity) {
    (void)capture;
    if (buffer != NULL && capacity > 0) buffer[0] = '\0';
    return 0;
}

uint64_t oink_audio_host_time_ns(void) {
    static mach_timebase_info_data_t timebase;
    if (timebase.denom == 0) mach_timebase_info(&timebase);
    uint64_t ticks = mach_absolute_time();
    return ticks * timebase.numer / timebase.denom;
}

int32_t oink_audio_capture_stop(void *capture, char *error, size_t capacity) {
    (void)capture;
    unavailable(error, capacity);
    return -1;
}

int32_t oink_audio_capture_check(void *capture, char *error, size_t capacity) {
    (void)capture;
    unavailable(error, capacity);
    return -1;
}

int32_t oink_audio_capture_format_health(int32_t status, bool supported, bool changed) {
    (void)supported;
    (void)changed;
    return status;
}

uint64_t oink_audio_capture_omitted_silent_frames(void *capture) {
    (void)capture;
    return 0;
}

void oink_audio_thread_scheduling(void *capture, int32_t *qos, int32_t *relative, int32_t *policy,
                                  int32_t *priority, int32_t *error) {
    (void)capture;
    if (qos != NULL) *qos = 0;
    if (relative != NULL) *relative = 0;
    if (policy != NULL) *policy = 0;
    if (priority != NULL) *priority = 0;
    if (error != NULL) *error = 0;
}

void *oink_audio_capture_test_silent_new(void) { return NULL; }

void oink_audio_capture_test_silent_callback(void *capture) { (void)capture; }

void oink_audio_capture_test_silent_free(void *capture) { (void)capture; }
