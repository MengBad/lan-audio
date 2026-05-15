#pragma once

#include <oboe/Oboe.h>

#include <array>
#include <atomic>
#include <cstdint>
#include <memory>
#include <mutex>

class PcmRingBuffer {
public:
    static constexpr int kChannels = 2;
    static constexpr int kCapacityFrames = 8192;  // stereo frames

    bool push(const int16_t *data, int frames);
    int pull(int16_t *out, int frames);
    int availableFrames() const;
    void reset();

private:
    std::array<int16_t, kCapacityFrames * kChannels> buffer_{};
    std::atomic<uint64_t> write_frame_pos_{0};
    std::atomic<uint64_t> read_frame_pos_{0};
};

// Phase 6.4 Hi-Res float ring buffer. Mirrors PcmRingBuffer's lock-free
// SPSC layout (single producer = decode thread, single consumer = Oboe
// callback) but holds 32-bit floats in [-1.0, 1.0]. Used only when the
// sink is opened in `openFloat()` for PCM24 passthrough.
//
// Sized for high native rates: at 384 kHz the i16 buffer's 8192-frame
// capacity yields only ~21 ms which is below typical jitter-batch
// (~10 ms × 2-3 frames) budget and triggers backpressure. 65536 frames
// = 170 ms at 384 kHz / 683 ms at 96 kHz, leaving comfortable headroom.
class PcmFloatRingBuffer {
public:
    static constexpr int kChannels = 2;
    static constexpr int kCapacityFrames = 65536;

    bool push(const float *data, int frames);
    int pull(float *out, int frames);
    int availableFrames() const;
    void reset();

private:
    std::array<float, kCapacityFrames * kChannels> buffer_{};
    std::atomic<uint64_t> write_frame_pos_{0};
    std::atomic<uint64_t> read_frame_pos_{0};
};

// Single-channel biquad peaking filter (Audio EQ Cookbook, RBJ).
// Coefficients are recomputed whenever sample rate, gain, frequency or Q
// change. Coefficients are read-only during processing (set by the caller
// of setPeaking, then snapshotted by the producer thread). Delay state
// (z1/z2/y1/y2) is per-filter and mutates inside processSample.
class BiquadFilter {
public:
    void reset();
    // Configure as a peaking EQ band. `gain_db` may be 0 (flat).
    // `q` is bandwidth — 0.7 is a wide musical peaking band, 1.0 is
    // narrower. Sample rate is in Hz. Frequencies above Nyquist are
    // clamped silently to a flat passthrough.
    void setPeaking(double sample_rate_hz, double center_hz, double gain_db, double q);
    inline float processSample(float in) {
        const double y = b0_ * in + b1_ * z1_ + b2_ * z2_ - a1_ * y1_ - a2_ * y2_;
        z2_ = z1_;
        z1_ = static_cast<double>(in);
        y2_ = y1_;
        y1_ = y;
        return static_cast<float>(y);
    }
    // Read the coefficients into a plain struct so the producer can
    // snapshot them under a short lock and then process out-of-lock.
    struct Coefs {
        double b0, b1, b2, a1, a2;
    };
    Coefs coefs() const {
        return Coefs{b0_, b1_, b2_, a1_, a2_};
    }

private:
    // Direct Form I.
    double b0_ = 1.0, b1_ = 0.0, b2_ = 0.0;
    double a1_ = 0.0, a2_ = 0.0;
    double z1_ = 0.0, z2_ = 0.0;
    double y1_ = 0.0, y2_ = 0.0;
};

class OboeAudioSink : public oboe::AudioStreamDataCallback,
                      public oboe::AudioStreamErrorCallback {
public:
    bool open(int sample_rate, int channel_count);
    // Phase 6.4 Hi-Res passthrough. Opens the Oboe stream in
    // `AudioFormat::Float` and routes audio through the float ring
    // buffer + float EQ path. PCM16 path is unaffected; only one path
    // is active per stream lifetime.
    bool openFloat(int sample_rate, int channel_count);
    void close();
    bool pushPcm(const int16_t *data, int frames);
    // Phase 6.4. Accepts raw big-endian 24-bit signed integer samples
    // from the wire (server emits BE per the v3 spec). Decodes into
    // float [-1.0, 1.0] in-place at the JNI boundary, then enters the
    // float ring buffer. Caller must have opened the sink with
    // `openFloat()` — calling this on an i16-opened sink returns false.
    bool pushPcm24Be(const uint8_t *data, int frames);
    int getSilenceFillCount() const;
    int getUnderrunCount();
    int getRingBufferLevelFrames() const;
    uint64_t getCallbackCount() const;
    int channelCount() const;
    bool isFloatPath() const { return is_float_path_; }

    // Phase 6 EQ: set the 3-band peaking equalizer state. Frequencies are
    // hard-coded to 60 Hz / 1 kHz / 10 kHz to match the Flutter UI labels.
    // `*_db` values are clamped server-side to [-10, +10] dB. When
    // `enabled=false` the filters are bypassed without re-computing
    // coefficients; toggling on/off is therefore zero-cost.
    void setEqSettings(bool enabled, int low_db, int mid_db, int high_db);

    oboe::DataCallbackResult onAudioReady(
        oboe::AudioStream *stream,
        void *audioData,
        int32_t numFrames) override;

    void onErrorAfterClose(
        oboe::AudioStream *stream,
        oboe::Result error) override;

private:
    bool reopenStream();
    void closeLocked();
    // Phase 6.4 stutter fix: starts the float Oboe stream once the
    // ring has accumulated `prestart_target_frames_` pre-buffered
    // frames. Idempotent — calling it after the stream is already
    // running is a no-op. Holds `stream_mutex_` is NOT required (the
    // caller is `pushPcm24Be` which is the producer thread, and the
    // stream pointer + counters in this helper are only mutated
    // here and during open/close which the caller already serializes
    // against).
    void tryStartDeferredFloat();
    // Apply the configured 3-band cascade to one interleaved PCM16 buffer
    // in place. No-op when EQ is disabled. Safe to call on the producer
    // thread because `setEqSettings` writes coefficients under
    // `eq_state_mutex_`, and processPcmInPlace takes a copy of the
    // coefficient bank under the same lock before doing the math.
    void processPcmInPlace(int16_t *data, int frames);
    // Phase 6.4 float-domain EQ. Same algorithm as the i16 path but
    // works on float [-1.0, 1.0] samples without the post-clamp; range
    // is preserved into the ring buffer. PCM16 path is unchanged.
    void processPcmFloatInPlace(float *data, int frames);
    void rebuildEqFilters(int sample_rate_hz, int low_db, int mid_db, int high_db);

    mutable std::mutex stream_mutex_;
    std::shared_ptr<oboe::AudioStream> stream_;
    PcmRingBuffer ring_buffer_;
    PcmFloatRingBuffer ring_buffer_float_;
    bool is_float_path_ = false;
    // Phase 6.4 stutter fix: defer requestStart() until the float ring
    // has accumulated `prestart_target_frames_` frames. Prevents the
    // race where AAudio's first callback fires before any audio has
    // been pushed, which causes 50+ silence-fill events/sec on
    // high-frequency content. Set in openFloat(), cleared on first
    // successful auto-start.
    bool float_prestart_pending_ = false;
    int prestart_target_frames_ = 0;
    std::atomic<int> silence_fill_count_{0};
    std::atomic<int> last_underrun_count_{0};
    std::atomic<int> reopen_attempts_{0};
    std::atomic<uint64_t> callback_count_{0};
    std::atomic<uint64_t> last_callback_log_ns_{0};
    int sample_rate_ = 48000;
    int channel_count_ = 2;

    // Phase 6 EQ state. The producer thread owns the filter bank
    // (delay state stays attached to filters across pushPcm calls). The
    // method-channel thread writes new gain values into the "pending"
    // bank under `eq_state_mutex_`; the producer picks them up at the
    // next pushPcm boundary.
    mutable std::mutex eq_state_mutex_;
    std::atomic<bool> eq_enabled_{false};
    std::atomic<bool> eq_pending_dirty_{false};
    int eq_low_db_ = 0;
    int eq_mid_db_ = 0;
    int eq_high_db_ = 0;
    // Producer-owned filter bank. Holds delay state across pushPcm calls.
    // 3 bands × up to 2 channels = 6 filters. Index = band * channels + ch.
    std::array<BiquadFilter, 3 * PcmRingBuffer::kChannels> eq_filters_{};
};
