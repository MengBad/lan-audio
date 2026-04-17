package com.example.lan_audio_android_mvp

import java.util.TreeMap

data class PcmFrame(
    val sequence: Int,
    val payload: ByteArray,
    val sampleRate: Int,
    val channels: Int,
    val frameDurationMs: Int,
)

data class JitterStats(
    var bufferedFrames: Int = 0,
    var underrunCount: Int = 0,
    var droppedFrames: Int = 0,
    var lateFrames: Int = 0,
)

class PlaybackJitterBuffer(
    private val startBufferMs: Int,
    private val maxBufferMs: Int,
    private val dropThresholdMs: Int = maxBufferMs,
) {
    private val frames = TreeMap<Int, PcmFrame>(::compareSeq)
    private var playoutStarted = false
    private var expectedSequence: Int? = null
    private var frameDurationMs = 10

    val stats = JitterStats()

    fun clear() {
        frames.clear()
        playoutStarted = false
        expectedSequence = null
        frameDurationMs = 10
        stats.bufferedFrames = 0
        stats.underrunCount = 0
        stats.droppedFrames = 0
        stats.lateFrames = 0
    }

    fun push(packet: LasPacket) {
        frameDurationMs = packet.frameDurationMs.takeIf { it > 0 } ?: 10
        val frame = PcmFrame(
            sequence = packet.sequence,
            payload = packet.payload,
            sampleRate = packet.sampleRate,
            channels = packet.channels,
            frameDurationMs = frameDurationMs,
        )

        if (frames.containsKey(frame.sequence)) {
            stats.droppedFrames += 1
            return
        }
        val expected = expectedSequence
        if (expected != null && isOlder(frame.sequence, expected)) {
            stats.lateFrames += 1
            return
        }

        frames[frame.sequence] = frame
        trimIfNeeded()
        trimLatencyTailIfNeeded()
        stats.bufferedFrames = frames.size
    }

    fun pop(): PcmFrame? {
        if (!playoutStarted) {
            val startFrames = kotlin.math.ceil(startBufferMs / frameDurationMs.toDouble()).toInt()
            if (frames.size < startFrames) {
                stats.bufferedFrames = frames.size
                return null
            }
            playoutStarted = true
            expectedSequence = frames.firstKey()
        }

        val expected = expectedSequence ?: return null
        val frame = frames.remove(expected)
        if (frame != null) {
            expectedSequence = nextSeq(expected)
            stats.bufferedFrames = frames.size
            return frame
        }

        val oldest = frames.firstKeyOrNull()
        if (oldest == null) {
            stats.underrunCount += 1
            playoutStarted = false
            expectedSequence = null
            stats.bufferedFrames = 0
            return null
        }

        val gap = ((oldest.toLong() - expected.toLong()) and 0xFFFFFFFFL).toInt()
        if (gap > 0) {
            stats.droppedFrames += gap
        }
        val recovered = frames.remove(oldest)
        if (recovered == null) {
            stats.underrunCount += 1
            expectedSequence = nextSeq(expected)
            stats.bufferedFrames = frames.size
            return null
        }
        expectedSequence = nextSeq(oldest)
        stats.bufferedFrames = frames.size
        return recovered
    }

    fun bufferedMs(): Int = frames.size * frameDurationMs

    private fun trimIfNeeded() {
        val maxFrames = kotlin.math.ceil(maxBufferMs / frameDurationMs.toDouble()).toInt()
            .coerceIn(8, 2000)
        while (frames.size > maxFrames) {
            frames.pollFirstEntry()
            stats.droppedFrames += 1
        }
    }

    private fun trimLatencyTailIfNeeded() {
        val thresholdFrames = kotlin.math.ceil(dropThresholdMs / frameDurationMs.toDouble()).toInt()
            .coerceAtLeast(1)
        val targetFrames = kotlin.math.ceil(startBufferMs / frameDurationMs.toDouble()).toInt()
            .coerceAtLeast(1)
        if (frames.size <= thresholdFrames) {
            return
        }
        while (frames.size > targetFrames) {
            frames.pollFirstEntry()
            stats.droppedFrames += 1
        }
        playoutStarted = false
        expectedSequence = frames.firstKeyOrNull()
    }

    private fun compareSeq(a: Int, b: Int): Int {
        if (a == b) return 0
        val diff = ((a.toLong() - b.toLong()) and 0xFFFFFFFFL)
        return if (diff < 0x80000000L) 1 else -1
    }

    private fun isOlder(a: Int, b: Int): Boolean {
        val diff = ((a.toLong() - b.toLong()) and 0xFFFFFFFFL)
        return diff > 0x80000000L
    }

    private fun nextSeq(seq: Int): Int {
        return ((seq.toLong() + 1L) and 0xFFFFFFFFL).toInt()
    }
}

private fun <K, V> TreeMap<K, V>.firstKeyOrNull(): K? = if (isEmpty()) null else firstKey()
