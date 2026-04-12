package com.example.lan_audio_android_mvp

import java.nio.ByteBuffer
import java.nio.ByteOrder

data class LasPacket(
    val sequence: Int,
    val timestampMs: Long,
    val sampleRate: Int,
    val channels: Int,
    val framesPerPacket: Int,
    val payload: ByteArray,
) {
    val frameDurationMs: Int
        get() = if (sampleRate <= 0) 0 else ((framesPerPacket * 1000.0) / sampleRate).toInt()
}

object LasPacketParser {
    fun parse(bytes: ByteArray): LasPacket? {
        if (bytes.size < 27) {
            return null
        }
        if (bytes[0].toInt() != 'L'.code ||
            bytes[1].toInt() != 'A'.code ||
            bytes[2].toInt() != 'S'.code ||
            bytes[3].toInt() != '1'.code
        ) {
            return null
        }

        val bb = ByteBuffer.wrap(bytes).order(ByteOrder.LITTLE_ENDIAN)
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
            sequence = sequence,
            timestampMs = timestampMs,
            sampleRate = sampleRate,
            channels = channels,
            framesPerPacket = framesPerPacket,
            payload = payload,
        )
    }
}
