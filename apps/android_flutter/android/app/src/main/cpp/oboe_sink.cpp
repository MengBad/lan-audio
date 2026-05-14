#include "oboe_sink.h"

#include <android/log.h>

#include <algorithm>
#include <ctime>
#include <cstring>

namespace {
constexpr const char *kTag = "lan_audio_oboe";
// Number of frames to fade out before a silence fill to avoid pops/clicks.
// At 48kHz this is ~1ms — short enough to be inaudible as a fade but enough
// to eliminate the hard discontinuity.
constexpr int kFadeRampFrames = 48;

void copy_samples(const int16_t *source, int16_t *dest, int sample_count) {
    if (sample_count <= 0) {
        return;
    }
    std::memcpy(dest, source, static_cast<size_t>(sample_count) * sizeof(int16_t));
}
}  // namespace

bool PcmRingBuffer::push(const int16_t *data, int frames) {
    if (data == nullptr || frames <= 0 || frames > kCapacityFrames) {
        return false;
    }
    const auto write_pos = write_frame_pos_.load(std::memory_order_relaxed);
    const auto read_pos = read_frame_pos_.load(std::memory_order_acquire);
    const auto used_frames = static_cast<int>(write_pos - read_pos);
    const auto free_frames = kCapacityFrames - used_frames;
    if (free_frames < frames) {
        return false;
    }

    const int write_start_frame = static_cast<int>(write_pos % kCapacityFrames);
    const int first_chunk_frames = std::min(frames, kCapacityFrames - write_start_frame);
    const int write_start_sample = write_start_frame * kChannels;
    const int first_chunk_samples = first_chunk_frames * kChannels;
    copy_samples(data, buffer_.data() + write_start_sample, first_chunk_samples);
    if (frames > first_chunk_frames) {
        const int remain_frames = frames - first_chunk_frames;
        copy_samples(data + first_chunk_samples, buffer_.data(), remain_frames * kChannels);
    }
    write_frame_pos_.store(write_pos + static_cast<uint64_t>(frames), std::memory_order_release);
    return true;
}

int PcmRingBuffer::pull(int16_t *out, int frames) {
    if (out == nullptr || frames <= 0) {
        return 0;
    }
    const auto read_pos = read_frame_pos_.load(std::memory_order_relaxed);
    const auto write_pos = write_frame_pos_.load(std::memory_order_acquire);
    const auto available_frames =
        static_cast<int>(std::min<uint64_t>(write_pos - read_pos, static_cast<uint64_t>(frames)));
    if (available_frames <= 0) {
        return 0;
    }

    const int read_start_frame = static_cast<int>(read_pos % kCapacityFrames);
    const int first_chunk_frames = std::min(available_frames, kCapacityFrames - read_start_frame);
    const int read_start_sample = read_start_frame * kChannels;
    const int first_chunk_samples = first_chunk_frames * kChannels;
    copy_samples(buffer_.data() + read_start_sample, out, first_chunk_samples);
    if (available_frames > first_chunk_frames) {
        const int remain_frames = available_frames - first_chunk_frames;
        copy_samples(buffer_.data(), out + first_chunk_samples, remain_frames * kChannels);
    }
    read_frame_pos_.store(read_pos + static_cast<uint64_t>(available_frames), std::memory_order_release);
    return available_frames;
}

int PcmRingBuffer::availableFrames() const {
    const auto read_pos = read_frame_pos_.load(std::memory_order_acquire);
    const auto write_pos = write_frame_pos_.load(std::memory_order_acquire);
    return static_cast<int>(write_pos - read_pos);
}

void PcmRingBuffer::reset() {
    read_frame_pos_.store(0, std::memory_order_release);
    write_frame_pos_.store(0, std::memory_order_release);
}

bool OboeAudioSink::open(int sample_rate, int channel_count) {
    std::lock_guard<std::mutex> lock(stream_mutex_);
    closeLocked();
    sample_rate_ = sample_rate;
    channel_count_ = channel_count;
    ring_buffer_.reset();
    silence_fill_count_.store(0, std::memory_order_release);
    last_underrun_count_.store(0, std::memory_order_release);
    reopen_attempts_.store(0, std::memory_order_release);
    callback_count_.store(0, std::memory_order_release);

    oboe::AudioStreamBuilder builder;
    builder.setDirection(oboe::Direction::Output);
    builder.setPerformanceMode(oboe::PerformanceMode::LowLatency);
    builder.setSharingMode(oboe::SharingMode::Exclusive);
    builder.setFormat(oboe::AudioFormat::I16);
    builder.setChannelCount(channel_count_);
    builder.setSampleRate(sample_rate_);
    builder.setDataCallback(this);
    builder.setErrorCallback(this);

    auto result = builder.openStream(stream_);
    if (result != oboe::Result::OK || !stream_) {
        __android_log_print(
            ANDROID_LOG_ERROR,
            kTag,
            "oboe openStream failed result=%s",
            oboe::convertToText(result));
        stream_.reset();
        return false;
    }

    const auto start_result = stream_->requestStart();
    if (start_result != oboe::Result::OK) {
        __android_log_print(
            ANDROID_LOG_ERROR,
            kTag,
            "oboe requestStart failed result=%s",
            oboe::convertToText(start_result));
        closeLocked();
        return false;
    }

    __android_log_print(
        ANDROID_LOG_INFO,
        kTag,
        "oboe stream opened sampleRate=%d channelCount=%d framesPerBurst=%d",
        stream_->getSampleRate(),
        stream_->getChannelCount(),
        stream_->getFramesPerBurst());
    return true;
}

void OboeAudioSink::close() {
    std::lock_guard<std::mutex> lock(stream_mutex_);
    closeLocked();
}

bool OboeAudioSink::pushPcm(const int16_t *data, int frames) {
    return ring_buffer_.push(data, frames);
}

int OboeAudioSink::getSilenceFillCount() const {
    return silence_fill_count_.load(std::memory_order_acquire);
}

int OboeAudioSink::getUnderrunCount() {
    std::lock_guard<std::mutex> lock(stream_mutex_);
    if (!stream_) {
        return last_underrun_count_.load(std::memory_order_acquire);
    }
    auto xrun = stream_->getXRunCount();
    if (xrun) {
        last_underrun_count_.store(std::max(0, xrun.value()), std::memory_order_release);
    }
    return last_underrun_count_.load(std::memory_order_acquire);
}

int OboeAudioSink::getRingBufferLevelFrames() const {
    return ring_buffer_.availableFrames();
}

uint64_t OboeAudioSink::getCallbackCount() const {
    return callback_count_.load(std::memory_order_acquire);
}

int OboeAudioSink::channelCount() const {
    return std::max(1, channel_count_);
}

oboe::DataCallbackResult OboeAudioSink::onAudioReady(
    oboe::AudioStream * /*stream*/,
    void *audioData,
    int32_t numFrames) {
    auto *out = static_cast<int16_t *>(audioData);
    const int requested_frames = std::max(0, static_cast<int>(numFrames));
    const int channels = std::max(1, channel_count_);
    const int got_frames = ring_buffer_.pull(out, requested_frames);
    if (got_frames < requested_frames) {
        const int missing_frames = requested_frames - got_frames;
        const int got_samples = got_frames * channels;
        // Apply a short fade-out ramp on the last available samples to avoid a hard
        // transition to silence (which causes audible pops/clicks).
        const int ramp_frames = std::min(got_frames, kFadeRampFrames);
        if (ramp_frames > 0) {
            const int ramp_start_sample = (got_frames - ramp_frames) * channels;
            for (int i = 0; i < ramp_frames; ++i) {
                const float gain = 1.0f - static_cast<float>(i + 1) / static_cast<float>(ramp_frames + 1);
                for (int ch = 0; ch < channels; ++ch) {
                    const int idx = ramp_start_sample + i * channels + ch;
                    out[idx] = static_cast<int16_t>(static_cast<float>(out[idx]) * gain);
                }
            }
        }
        std::memset(out + got_samples, 0, static_cast<size_t>(missing_frames * channels) * sizeof(int16_t));
        silence_fill_count_.fetch_add(1, std::memory_order_relaxed);
    }
    const auto cb = callback_count_.fetch_add(1, std::memory_order_relaxed) + 1;
    timespec now_ts{};
    clock_gettime(CLOCK_MONOTONIC, &now_ts);
    const uint64_t now_ns =
        static_cast<uint64_t>(now_ts.tv_sec) * 1000000000ULL + static_cast<uint64_t>(now_ts.tv_nsec);
    const uint64_t prev_ns = last_callback_log_ns_.load(std::memory_order_relaxed);
    if (prev_ns == 0 || now_ns - prev_ns >= 1000000000ULL) {
        last_callback_log_ns_.store(now_ns, std::memory_order_relaxed);
        const int available_frames = ring_buffer_.availableFrames();
        __android_log_print(
            ANDROID_LOG_INFO,
            kTag,
            "callback_sample callback=%llu available_frames=%d requested_frames=%d pulled_frames=%d silence_total=%d",
            static_cast<unsigned long long>(cb),
            available_frames,
            requested_frames,
            got_frames,
            silence_fill_count_.load(std::memory_order_relaxed));
    }
    return oboe::DataCallbackResult::Continue;
}

void OboeAudioSink::onErrorAfterClose(
    oboe::AudioStream * /*stream*/,
    oboe::Result error) {
    const auto attempts = reopen_attempts_.fetch_add(1, std::memory_order_relaxed) + 1;
    __android_log_print(
        ANDROID_LOG_WARN,
        kTag,
        "oboe onErrorAfterClose error=%s reopen_attempt=%d",
        oboe::convertToText(error),
        attempts);
    if (attempts > 3) {
        return;
    }
    reopenStream();
}

bool OboeAudioSink::reopenStream() {
    std::lock_guard<std::mutex> lock(stream_mutex_);
    closeLocked();

    oboe::AudioStreamBuilder builder;
    builder.setDirection(oboe::Direction::Output);
    builder.setPerformanceMode(oboe::PerformanceMode::LowLatency);
    builder.setSharingMode(oboe::SharingMode::Exclusive);
    builder.setFormat(oboe::AudioFormat::I16);
    builder.setChannelCount(channel_count_);
    builder.setSampleRate(sample_rate_);
    builder.setDataCallback(this);
    builder.setErrorCallback(this);

    auto result = builder.openStream(stream_);
    if (result != oboe::Result::OK || !stream_) {
        __android_log_print(
            ANDROID_LOG_ERROR,
            kTag,
            "oboe reopen failed result=%s",
            oboe::convertToText(result));
        stream_.reset();
        return false;
    }
    const auto start_result = stream_->requestStart();
    if (start_result != oboe::Result::OK) {
        __android_log_print(
            ANDROID_LOG_ERROR,
            kTag,
            "oboe reopen requestStart failed result=%s",
            oboe::convertToText(start_result));
        closeLocked();
        return false;
    }
    return true;
}

void OboeAudioSink::closeLocked() {
    if (stream_) {
        stream_->requestStop();
        stream_->close();
        stream_.reset();
    }
    ring_buffer_.reset();
}
