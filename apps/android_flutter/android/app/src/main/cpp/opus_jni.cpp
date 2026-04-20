#include <jni.h>
#include <android/log.h>
#include <cmath>
#include <cstdint>
#include <vector>

#include "opus.h"

namespace {
constexpr const char *kTag = "lan_audio_opus_jni";

struct DecoderHandle {
    OpusDecoder *decoder = nullptr;
    int sample_rate = 0;
    int channels = 0;
};

DecoderHandle *from_handle(jlong handle) {
    return reinterpret_cast<DecoderHandle *>(handle);
}

jint throw_illegal_state(JNIEnv *env, const char *message) {
    jclass cls = env->FindClass("java/lang/IllegalStateException");
    if (cls != nullptr) {
        env->ThrowNew(cls, message);
    }
    return -1;
}
}  // namespace

extern "C" JNIEXPORT jboolean JNICALL
Java_com_example_lan_1audio_1android_1mvp_OpusNativeDecoder_nativeIsAvailable(
    JNIEnv *, jobject) {
    return JNI_TRUE;
}

extern "C" JNIEXPORT jlong JNICALL
Java_com_example_lan_1audio_1android_1mvp_OpusNativeDecoder_nativeCreate(
    JNIEnv *env, jobject, jint sample_rate, jint channels) {
    if (sample_rate <= 0 || channels <= 0 || channels > 2) {
        throw_illegal_state(env, "invalid opus decoder format");
        return 0;
    }

    int error = OPUS_OK;
    OpusDecoder *decoder = opus_decoder_create(sample_rate, channels, &error);
    if (error != OPUS_OK || decoder == nullptr) {
        __android_log_print(ANDROID_LOG_ERROR, kTag, "opus_decoder_create failed: %s", opus_strerror(error));
        throw_illegal_state(env, opus_strerror(error));
        return 0;
    }

    auto *handle = new DecoderHandle();
    handle->decoder = decoder;
    handle->sample_rate = sample_rate;
    handle->channels = channels;
    return reinterpret_cast<jlong>(handle);
}

extern "C" JNIEXPORT void JNICALL
Java_com_example_lan_1audio_1android_1mvp_OpusNativeDecoder_nativeDestroy(
    JNIEnv *, jobject, jlong handle_value) {
    DecoderHandle *handle = from_handle(handle_value);
    if (handle == nullptr) {
        return;
    }
    if (handle->decoder != nullptr) {
        opus_decoder_destroy(handle->decoder);
        handle->decoder = nullptr;
    }
    delete handle;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_example_lan_1audio_1android_1mvp_OpusNativeDecoder_nativeDecode(
    JNIEnv *env,
    jobject,
    jlong handle_value,
    jbyteArray packet,
    jint offset,
    jint length,
    jshortArray pcm_out,
    jint max_frames,
    jboolean use_plc) {
    DecoderHandle *handle = from_handle(handle_value);
    if (handle == nullptr || handle->decoder == nullptr) {
        return throw_illegal_state(env, "opus decoder is closed");
    }
    if (pcm_out == nullptr || offset < 0 || length < 0 || max_frames <= 0) {
        return throw_illegal_state(env, "invalid opus decode input");
    }

    const jsize out_size = env->GetArrayLength(pcm_out);
    if (out_size < max_frames * handle->channels) {
        return throw_illegal_state(env, "opus pcm output buffer is too small");
    }

    std::vector<unsigned char> encoded;
    const unsigned char *encoded_ptr = nullptr;
    int encoded_length = 0;
    if (!use_plc) {
        if (packet == nullptr || length <= 0) {
            return throw_illegal_state(env, "opus packet is required when PLC is disabled");
        }
        const jsize packet_size = env->GetArrayLength(packet);
        if (offset + length > packet_size) {
            return throw_illegal_state(env, "opus packet range is out of bounds");
        }
        encoded.resize(static_cast<size_t>(length));
        env->GetByteArrayRegion(packet, offset, length, reinterpret_cast<jbyte *>(encoded.data()));
        if (env->ExceptionCheck()) {
            return -1;
        }
        encoded_ptr = encoded.data();
        encoded_length = length;
    }

    std::vector<opus_int16> decoded(static_cast<size_t>(max_frames * handle->channels));
    const int frames = opus_decode(
        handle->decoder,
        encoded_ptr,
        encoded_length,
        decoded.data(),
        max_frames,
        0);
    if (frames < 0) {
        __android_log_print(ANDROID_LOG_WARN, kTag, "opus_decode failed: %s", opus_strerror(frames));
        return frames;
    }

    env->SetShortArrayRegion(pcm_out, 0, frames * handle->channels, decoded.data());
    return frames;
}

extern "C" JNIEXPORT jint JNICALL
Java_com_example_lan_1audio_1android_1mvp_OpusNativeDecoder_nativeSelfTestDecodePeak(
    JNIEnv *env,
    jobject,
    jint sample_rate,
    jint channels) {
    if (sample_rate <= 0 || channels <= 0 || channels > 2) {
        return throw_illegal_state(env, "invalid opus self-test format");
    }

    constexpr int kFrameMs = 10;
    const int frames = sample_rate * kFrameMs / 1000;
    const int samples = frames * channels;
    std::vector<opus_int16> input(static_cast<size_t>(samples));
    for (int frame = 0; frame < frames; ++frame) {
        const double phase = static_cast<double>(frame) * 440.0 * 2.0 * M_PI / sample_rate;
        const auto sample = static_cast<opus_int16>(std::sin(phase) * 6000.0);
        for (int ch = 0; ch < channels; ++ch) {
            input[frame * channels + ch] = sample;
        }
    }

    int error = OPUS_OK;
    OpusEncoder *encoder = opus_encoder_create(sample_rate, channels, OPUS_APPLICATION_AUDIO, &error);
    if (error != OPUS_OK || encoder == nullptr) {
        return throw_illegal_state(env, opus_strerror(error));
    }

    std::vector<unsigned char> packet(4000);
    const int encoded_size = opus_encode(encoder, input.data(), frames, packet.data(), static_cast<opus_int32>(packet.size()));
    opus_encoder_destroy(encoder);
    if (encoded_size < 0) {
        return throw_illegal_state(env, opus_strerror(encoded_size));
    }

    OpusDecoder *decoder = opus_decoder_create(sample_rate, channels, &error);
    if (error != OPUS_OK || decoder == nullptr) {
        return throw_illegal_state(env, opus_strerror(error));
    }

    std::vector<opus_int16> decoded(static_cast<size_t>(samples));
    const int decoded_frames = opus_decode(decoder, packet.data(), encoded_size, decoded.data(), frames, 0);
    opus_decoder_destroy(decoder);
    if (decoded_frames < 0) {
        return throw_illegal_state(env, opus_strerror(decoded_frames));
    }

    int peak = 0;
    const int decoded_samples = decoded_frames * channels;
    for (int idx = 0; idx < decoded_samples; ++idx) {
        const int value = std::abs(static_cast<int>(decoded[idx]));
        if (value > peak) {
            peak = value;
        }
    }
    return peak;
}
