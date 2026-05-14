package com.example.lan_audio_android_mvp

import java.nio.ByteBuffer
import java.nio.ByteOrder

data class LasPacket(
    val protocolVersion: Int,
    val headerSize: Int,
    val flags: Int,
    val sequence: Int,
    val timestampMs: Long,
    val codec: Int,
    val sampleRate: Int,
    val channels: Int,
    val framesPerPacket: Int,
    val payload: ByteArray,
    // Phase 6 v3 fragmentation. For v1/v2 packets these are always
    // (0, 1, sequence) — i.e. "single fragment, identity logical_seq".
    // For v3 + PCM24 the values come from the wire; the consumer is
    // expected to use the LasPacketReassembler to rebuild the logical
    // payload before decoding.
    val fragIndex: Int = 0,
    val totalFrags: Int = 1,
    val logicalSeq: Long = 0,
) {
    companion object {
        const val CODEC_PCM16 = 1
        const val CODEC_OPUS_EXPERIMENTAL = 3
        const val CODEC_PCM24 = 4
    }

    val frameDurationMs: Int
        get() = if (sampleRate <= 0) 0 else ((framesPerPacket * 1000.0) / sampleRate).toInt()

    val codecLabel: String
        get() = when (codec) {
            CODEC_OPUS_EXPERIMENTAL -> "opus"
            CODEC_PCM24 -> "pcm24"
            else -> "pcm16"
        }

    val hasConfigChanged: Boolean
        get() = (flags and 0x02) != 0

    val hasDiscontinuity: Boolean
        get() = (flags and 0x04) != 0

    val isFragmented: Boolean
        get() = totalFrags > 1
}

object LasPacketParser {
    fun parse(bytes: ByteArray): LasPacket? {
        if (bytes.size < 4) {
            return null
        }
        val magic = String(bytes.copyOfRange(0, 4), Charsets.US_ASCII)
        return when (magic) {
            "LAS1" -> parseLegacy(bytes)
            "LAV2" -> parseV2(bytes)
            else -> null
        }
    }

    private fun parseLegacy(bytes: ByteArray): LasPacket? {
        if (bytes.size < 27) {
            return null
        }

        val bb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val protocolVersion = bb.get(4).toInt() and 0xFF
        val flags = bb.get(5).toInt() and 0xFF
        val sequence = bb.getInt(6)
        val timestampMs = bb.getLong(10)
        val sampleRate = bb.getInt(18)
        val channels = bb.get(22).toInt() and 0xFF
        val framesPerPacket = bb.getShort(23).toInt() and 0xFFFF
        val payloadLen = bb.getShort(25).toInt() and 0xFFFF
        if (bytes.size != 27 + payloadLen) {
            return null
        }

        val payload = bytes.copyOfRange(27, bytes.size)
        return LasPacket(
            protocolVersion = protocolVersion,
            headerSize = 27,
            flags = flags,
            sequence = sequence,
            timestampMs = timestampMs,
            codec = LasPacket.CODEC_PCM16,
            sampleRate = sampleRate,
            channels = channels,
            framesPerPacket = framesPerPacket,
            payload = payload,
        )
    }

    private fun parseV2(bytes: ByteArray): LasPacket? {
        if (bytes.size < 33) {
            return null
        }
        val bb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
        val protocolVersion = bb.get(4).toInt() and 0xFF
        return when (protocolVersion) {
            2 -> parseV2Inner(bb, bytes)
            3 -> parseV3Inner(bb, bytes)
            else -> null
        }
    }

    private fun parseV2Inner(bb: ByteBuffer, bytes: ByteArray): LasPacket? {
        val headerSize = bb.getShort(5).toInt() and 0xFFFF
        if (headerSize < 33 || bytes.size < headerSize) {
            return null
        }
        val flags = bb.getShort(7).toInt() and 0xFFFF
        val sequence = bb.getInt(9)
        val timestampMs = bb.getLong(13)
        val codec = bb.get(21).toInt() and 0xFF
        if (codec != LasPacket.CODEC_PCM16 && codec != LasPacket.CODEC_OPUS_EXPERIMENTAL) {
            return null
        }
        val channels = bb.get(22).toInt() and 0xFF
        val sampleRate = bb.getInt(23)
        val frameDurationMs = bb.getShort(27).toInt() and 0xFFFF
        val payloadLen = bb.getShort(29).toInt() and 0xFFFF
        if (bytes.size != headerSize + payloadLen) {
            return null
        }
        val framesPerPacket =
            if (sampleRate <= 0 || frameDurationMs <= 0) 0 else sampleRate * frameDurationMs / 1000
        val payload = bytes.copyOfRange(headerSize, bytes.size)
        return LasPacket(
            protocolVersion = 2,
            headerSize = headerSize,
            flags = flags,
            sequence = sequence,
            timestampMs = timestampMs,
            codec = codec,
            sampleRate = sampleRate,
            channels = channels,
            framesPerPacket = framesPerPacket,
            payload = payload,
            fragIndex = 0,
            totalFrags = 1,
            logicalSeq = sequence.toLong() and 0xFFFFFFFFL,
        )
    }

    /// Phase 6 v3 parser. Same wire layout as v2 for the first 31 bytes,
    /// then the v2 `reserved` u16 slot is repurposed as
    /// `frag_index_u8 | total_frags_u8` (LE), followed by 4 bytes of
    /// `logical_seq` u32. Total header size = 37 bytes.
    private fun parseV3Inner(bb: ByteBuffer, bytes: ByteArray): LasPacket? {
        val headerSize = bb.getShort(5).toInt() and 0xFFFF
        if (headerSize != 37 || bytes.size < headerSize) {
            return null
        }
        val flags = bb.getShort(7).toInt() and 0xFFFF
        val sequence = bb.getInt(9)
        val timestampMs = bb.getLong(13)
        val codec = bb.get(21).toInt() and 0xFF
        // v3 must carry a known codec; PCM24 is the headline use case but
        // a v3 server may also send other codecs through the same path.
        if (codec != LasPacket.CODEC_PCM16 &&
            codec != LasPacket.CODEC_OPUS_EXPERIMENTAL &&
            codec != LasPacket.CODEC_PCM24
        ) {
            return null
        }
        val channels = bb.get(22).toInt() and 0xFF
        val sampleRate = bb.getInt(23)
        val frameDurationMs = bb.getShort(27).toInt() and 0xFFFF
        val payloadLen = bb.getShort(29).toInt() and 0xFFFF
        val fragIndex = bb.get(31).toInt() and 0xFF
        val totalFrags = bb.get(32).toInt() and 0xFF
        if (totalFrags == 0 || fragIndex >= totalFrags) {
            return null
        }
        val logicalSeq = bb.getInt(33).toLong() and 0xFFFFFFFFL
        if (bytes.size != headerSize + payloadLen) {
            return null
        }
        val framesPerPacket =
            if (sampleRate <= 0 || frameDurationMs <= 0) 0 else sampleRate * frameDurationMs / 1000
        val payload = bytes.copyOfRange(headerSize, bytes.size)
        return LasPacket(
            protocolVersion = 3,
            headerSize = headerSize,
            flags = flags,
            sequence = sequence,
            timestampMs = timestampMs,
            codec = codec,
            sampleRate = sampleRate,
            channels = channels,
            framesPerPacket = framesPerPacket,
            payload = payload,
            fragIndex = fragIndex,
            totalFrags = totalFrags,
            logicalSeq = logicalSeq,
        )
    }
}
