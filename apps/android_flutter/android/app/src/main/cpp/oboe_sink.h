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
    void close();
    bool pushPcm(const int16_t *data, int frames);
    int getSilenceFillCount() const;
    int getUnderrunCount();
    int getRingBufferLevelFrames() const;
    uint64_t getCallbackCount() const;
    int channelCount() const;

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
    // Apply the configured 3-band cascade to one interleaved PCM16 buffer
    // in place. No-op when EQ is disabled. Safe to call on the producer
    // thread because `setEqSettings` writes coefficients under
    // `eq_state_mutex_`, and processPcmInPlace takes a copy of the
    // coefficient bank under the same lock before doing the math.
    void processPcmInPlace(int16_t *data, int frames);
    void rebuildEqFilters(int sample_rate_hz, int low_db, int mid_db, int high_db);

    mutable std::mutex stream_mutex_;
    std::shared_ptr<oboe::AudioStream> stream_;
    PcmRingBuffer ring_buffer_;
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
