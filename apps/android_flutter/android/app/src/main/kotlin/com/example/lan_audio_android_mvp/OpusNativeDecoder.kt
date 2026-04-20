package com.example.lan_audio_android_mvp

import android.util.Log
import java.nio.ByteBuffer
import java.nio.ByteOrder
import kotlin.math.abs

class OpusNativeDecoder(
    private val sampleRate: Int,
    private val channels: Int,
) {
    private val logTag = "lan_audio_opus_jni"
    private var handle: Long = 0

    init {
        check(isAvailable()) { "libopus JNI decoder is not available" }
        handle = nativeCreate(sampleRate, channels)
        check(handle != 0L) { "failed to create libopus decoder" }
        check(nativeIsAvailable()) { "libopus native availability check failed" }
        Log.i(logTag, "libopus decoder created sampleRate=$sampleRate channels=$channels")
    }

    @Synchronized
    fun decodeToPcmBytes(packet: ByteArray, expectedFrames: Int): ByteArray? {
        check(handle != 0L) { "libopus decoder is closed" }
        val maxFrames = expectedFrames.coerceAtLeast(1)
        val pcm = ShortArray(maxFrames * channels.coerceAtLeast(1))
        val frames = nativeDecode(handle, packet, 0, packet.size, pcm, maxFrames)
        if (frames < 0) {
            Log.w(logTag, "libopus decode failed code=$frames")
            return null
        }
        if (frames == 0) {
            return null
        }

        val samples = frames * channels
        val out = ByteBuffer
            .allocate(samples * 2)
            .order(ByteOrder.LITTLE_ENDIAN)
        for (idx in 0 until samples) {
            out.putShort(pcm[idx])
        }
        return out.array()
    }

    @Synchronized
    fun release() {
        if (handle != 0L) {
            nativeDestroy(handle)
            handle = 0
        }
    }

    private external fun nativeIsAvailable(): Boolean
    private external fun nativeCreate(sampleRate: Int, channels: Int): Long
    private external fun nativeDecode(
        handle: Long,
        packet: ByteArray,
        offset: Int,
        length: Int,
        pcmOut: ShortArray,
        maxFrames: Int,
    ): Int
    private external fun nativeDestroy(handle: Long)
    private external fun nativeSelfTestDecodePeak(sampleRate: Int, channels: Int): Int

    fun selfTestDecodePeak(): Int = nativeSelfTestDecodePeak(sampleRate, channels)

    companion object {
        private const val TAG = "lan_audio_opus_jni"
        private var libraryLoadAttempted = false
        private var libraryLoaded = false

        @Synchronized
        fun isAvailable(): Boolean {
            if (!libraryLoadAttempted) {
                libraryLoadAttempted = true
                libraryLoaded = try {
                    System.loadLibrary("lan_audio_opus_jni")
                    true
                } catch (t: Throwable) {
                    Log.w(TAG, "libopus JNI decoder is unavailable", t)
                    false
                }
            }
            if (!libraryLoaded) {
                return false
            }
            return true
        }

        fun pcmPeak(pcmLittleEndian: ByteArray): Int {
            var peak = 0
            var idx = 0
            while (idx + 1 < pcmLittleEndian.size) {
                val lo = pcmLittleEndian[idx].toInt() and 0xff
                val hi = pcmLittleEndian[idx + 1].toInt()
                val sample = ((hi shl 8) or lo).toShort().toInt()
                peak = maxOf(peak, abs(sample))
                idx += 2
            }
            return peak
        }
    }
}
