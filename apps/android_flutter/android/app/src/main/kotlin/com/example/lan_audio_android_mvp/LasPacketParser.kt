package com.example.lan_audio_android_mvp

import java.nio.ByteBuffer
import java.nio.ByteOrder

data class LasPacket(
    val protocolVersion: Int,
    val headerSize: Int,
    val flags: Int,
    val sequence: Int,
    val timestampMs: Long,
    val sampleRate: Int,
    val channels: Int,
    val framesPerPacket: Int,
    val payload: ByteArray,
) {
    val frameDurationMs: Int
        get() = if (sampleRate <= 0) 0 else ((framesPerPacket * 1000.0) / sampleRate).toInt()

    val hasConfigChanged: Boolean
        get() = (flags and 0x02) != 0

    val hasDiscontinuity: Boolean
        get() = (flags and 0x04) != 0
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
        if (protocolVersion != 2) {
            return null
        }
        val headerSize = bb.getShort(5).toInt() and 0xFFFF
        if (headerSize < 33 || bytes.size < headerSize) {
            return null
        }
        val flags = bb.getShort(7).toInt() and 0xFFFF
        val sequence = bb.getInt(9)
        val timestampMs = bb.getLong(13)
        val codec = bb.get(21).toInt() and 0xFF
        if (codec != 1) {
            // Gray stage: only pcm16 is supported.
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
            protocolVersion = protocolVersion,
            headerSize = headerSize,
            flags = flags,
            sequence = sequence,
            timestampMs = timestampMs,
            sampleRate = sampleRate,
            channels = channels,
            framesPerPacket = framesPerPacket,
            payload = payload,
        )
    }
}
