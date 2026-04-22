package com.example.lan_audio_android_mvp

import android.util.Log

class OpusFrameDecoder {
    private val logTag = "lan_audio_opus"
    private var decoder: OpusNativeDecoder? = null
    private var sampleRate: Int = 0
    private var channels: Int = 0
    private var decodeFailures = 0L

    @Synchronized
    fun decode(packet: LasPacket): ByteArray? {
        require(packet.codec == LasPacket.CODEC_OPUS_EXPERIMENTAL) {
            "packet codec is not opus"
        }
        val nativeDecoder = ensureDecoder(packet.sampleRate, packet.channels)
        val expectedPcmBytes = packet.framesPerPacket.coerceAtLeast(1) *
            packet.channels.coerceAtLeast(1) *
            2

        val decoded = nativeDecoder.decodeToPcmBytes(packet.payload, packet.framesPerPacket)
        val recovered = decoded ?: nativeDecoder.decodeToPcmBytes(
            packet = ByteArray(0),
            expectedFrames = packet.framesPerPacket,
            usePlc = true,
        )
        if (recovered == null) {
            decodeFailures += 1
            if (decodeFailures == 1L || decodeFailures % 100L == 0L) {
                Log.w(logTag, "libopus decode+plc produced no PCM count=$decodeFailures")
            }
            return null
        }
        if (decoded == null) {
            Log.w(logTag, "libopus decode failed; PLC fallback supplied concealment frame")
        }

        val normalized = when {
            recovered.size == expectedPcmBytes -> recovered
            recovered.size > expectedPcmBytes -> recovered.copyOf(expectedPcmBytes)
            else -> {
                Log.w(
                    logTag,
                    "libopus decoded short frame decoded=${recovered.size}B expected=${expectedPcmBytes}B; padding with concealment tail",
                )
                recovered.copyOf(expectedPcmBytes)
            }
        }

        val peak = OpusNativeDecoder.pcmPeak(normalized)
        if (peak == 0) {
            decodeFailures += 1
            if (decodeFailures == 1L || decodeFailures % 100L == 0L) {
                Log.w(logTag, "libopus decoded PCM is silent count=$decodeFailures")
            }
        } else if (decodeFailures != 0L) {
            Log.i(logTag, "libopus decoded non-silent PCM after failures=$decodeFailures peak=$peak")
            decodeFailures = 0
        }
        return normalized
    }

    @Synchronized
    fun release() {
        decoder?.release()
        decoder = null
        sampleRate = 0
        channels = 0
        decodeFailures = 0
    }

    private fun ensureDecoder(nextSampleRate: Int, nextChannels: Int): OpusNativeDecoder {
        val current = decoder
        if (current != null && sampleRate == nextSampleRate && channels == nextChannels) {
            return current
        }
        release()
        val next = OpusNativeDecoder(nextSampleRate, nextChannels)
        decoder = next
        sampleRate = nextSampleRate
        channels = nextChannels
        Log.i(logTag, "libopus JNI decoder selected sampleRate=$nextSampleRate channels=$nextChannels")
        return next
    }

    companion object {
        fun isAvailable(): Boolean = OpusNativeDecoder.isAvailable()
    }
}
