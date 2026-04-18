package com.example.lan_audio_android_mvp

import android.media.AudioFormat
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaCodecList
import android.media.MediaFormat
import android.os.Build
import android.util.Log
import java.io.ByteArrayOutputStream
import java.nio.ByteBuffer
import java.nio.ByteOrder

class OpusFrameDecoder {
    private val logTag = "lan_audio_opus"
    private var codec: MediaCodec? = null
    private var sampleRate: Int = 0
    private var channels: Int = 0

    fun decode(packet: LasPacket): ByteArray {
        require(packet.codec == LasPacket.CODEC_OPUS_EXPERIMENTAL) {
            "packet codec is not opus_experimental"
        }
        val decoder = ensureDecoder(packet.sampleRate, packet.channels)
        val inputIndex = decoder.dequeueInputBuffer(2_000)
        if (inputIndex < 0) {
            throw IllegalStateException("opus decoder input buffer unavailable")
        }

        val inputBuffer = decoder.getInputBuffer(inputIndex)
            ?: throw IllegalStateException("opus decoder input buffer is null")
        inputBuffer.clear()
        inputBuffer.put(packet.payload)
        decoder.queueInputBuffer(
            inputIndex,
            0,
            packet.payload.size,
            packet.timestampMs * 1000L,
            0,
        )

        val info = MediaCodec.BufferInfo()
        val pcm = ByteArrayOutputStream(packet.framesPerPacket * packet.channels * 2)
        while (true) {
            when (val outputIndex = decoder.dequeueOutputBuffer(info, 2_000)) {
                MediaCodec.INFO_TRY_AGAIN_LATER -> break
                MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    Log.i(logTag, "opus output format=${decoder.outputFormat}")
                }
                MediaCodec.INFO_OUTPUT_BUFFERS_CHANGED -> {
                    // Deprecated signal, safe to ignore with getOutputBuffer().
                }
                else -> {
                    if (outputIndex >= 0) {
                        val outputBuffer = decoder.getOutputBuffer(outputIndex)
                        if (outputBuffer != null && info.size > 0) {
                            outputBuffer.position(info.offset)
                            outputBuffer.limit(info.offset + info.size)
                            val chunk = ByteArray(info.size)
                            outputBuffer.get(chunk)
                            pcm.write(chunk)
                        }
                        decoder.releaseOutputBuffer(outputIndex, false)
                    }
                }
            }
        }

        val out = pcm.toByteArray()
        if (out.isEmpty()) {
            throw IllegalStateException("opus decoder produced no PCM output")
        }
        return out
    }

    fun release() {
        codec?.let {
            try {
                it.stop()
            } catch (_: Throwable) {
            }
            try {
                it.release()
            } catch (_: Throwable) {
            }
        }
        codec = null
        sampleRate = 0
        channels = 0
    }

    private fun ensureDecoder(nextSampleRate: Int, nextChannels: Int): MediaCodec {
        val current = codec
        if (current != null && sampleRate == nextSampleRate && channels == nextChannels) {
            return current
        }
        release()

        val format = MediaFormat.createAudioFormat(OPUS_MIME, nextSampleRate, nextChannels)
        format.setByteBuffer("csd-0", opusHead(nextSampleRate, nextChannels))
        format.setByteBuffer("csd-1", longLittleEndianBuffer(0L))
        format.setByteBuffer("csd-2", longLittleEndianBuffer(80_000_000L))
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            format.setInteger(MediaFormat.KEY_PCM_ENCODING, AudioFormat.ENCODING_PCM_16BIT)
        }

        val decoder = MediaCodec.createDecoderByType(OPUS_MIME)
        decoder.configure(format, null, null, 0)
        decoder.start()
        codec = decoder
        sampleRate = nextSampleRate
        channels = nextChannels
        Log.i(logTag, "opus decoder started sampleRate=$nextSampleRate channels=$nextChannels")
        return decoder
    }

    companion object {
        private const val OPUS_MIME = "audio/opus"

        fun isAvailable(): Boolean {
            return try {
                val infos = MediaCodecList(MediaCodecList.ALL_CODECS).codecInfos
                infos.any { info: MediaCodecInfo ->
                    !info.isEncoder && info.supportedTypes.any { it.equals(OPUS_MIME, ignoreCase = true) }
                }
            } catch (_: Throwable) {
                false
            }
        }

        private fun opusHead(sampleRate: Int, channels: Int): ByteBuffer {
            val out = ByteBuffer.allocate(19).order(ByteOrder.LITTLE_ENDIAN)
            out.put("OpusHead".toByteArray(Charsets.US_ASCII))
            out.put(1)
            out.put(channels.toByte())
            out.putShort(0)
            out.putInt(sampleRate)
            out.putShort(0)
            out.put(0)
            out.flip()
            return out
        }

        private fun longLittleEndianBuffer(value: Long): ByteBuffer {
            val out = ByteBuffer.allocate(8).order(ByteOrder.LITTLE_ENDIAN)
            out.putLong(value)
            out.flip()
            return out
        }
    }
}
