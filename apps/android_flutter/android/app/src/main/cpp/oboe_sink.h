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
};
