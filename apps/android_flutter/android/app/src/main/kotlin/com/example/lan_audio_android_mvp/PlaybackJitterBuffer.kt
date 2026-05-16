package com.example.lan_audio_android_mvp

import android.os.SystemClock
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
    var floorHoldCount: Int = 0,
)

class PlaybackJitterBuffer(
    private var startBufferMs: Int,
    private var maxBufferMs: Int,
    private var dropThresholdMs: Int = maxBufferMs,
) {
    private val frames = TreeMap<Int, PcmFrame>(::compareSeq)
    private var playoutStarted = false
    private var expectedSequence: Int? = null
    private var frameDurationMs = 10
    private var overflowSinceMs: Long = 0

    val stats = JitterStats()

    @Synchronized
    fun clear() {
        frames.clear()
        playoutStarted = false
        expectedSequence = null
        frameDurationMs = 10
        overflowSinceMs = 0
        stats.bufferedFrames = 0
        stats.underrunCount = 0
        stats.droppedFrames = 0
        stats.lateFrames = 0
        stats.floorHoldCount = 0
    }

    @Synchronized
    fun reconfigure(startBufferMs: Int, maxBufferMs: Int, dropThresholdMs: Int = maxBufferMs) {
        this.startBufferMs = startBufferMs.coerceAtLeast(1)
        this.maxBufferMs = maxBufferMs.coerceAtLeast(this.startBufferMs)
        this.dropThresholdMs = dropThresholdMs.coerceAtLeast(this.startBufferMs)
        trimIfNeeded()
        trimLatencyTailIfNeeded()
        stats.bufferedFrames = frames.size
    }

    @Synchronized
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

    @Synchronized
    fun pop(usbLowLatencyHardFloor: Boolean = false): PcmFrame? {
        if (!playoutStarted) {
            val startFrames = kotlin.math.ceil(startBufferMs / frameDurationMs.toDouble()).toInt()
            if (frames.size < startFrames) {
                stats.bufferedFrames = frames.size
                return null
            }
            playoutStarted = true
            expectedSequence = frames.firstKey()
        }

        if (usbLowLatencyHardFloor && bufferedMs() <= HARD_FLOOR_MS) {
            stats.floorHoldCount += 1
            stats.bufferedFrames = frames.size
            return null
        }

        val expected = expectedSequence ?: return null
        val frame = frames.remove(expected)
        if (frame != null) {
            expectedSequence = nextSeq(expected)
            stats.bufferedFrames = frames.size
            return frame
        }

        val oldest = frames.firstEntry()?.key
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

    // Used by playout batching to fetch only immediately available contiguous frames.
    // Unlike pop(), this helper does not mark underrun or reset playout state when
    // the next expected frame is not available.
    @Synchronized
    fun popContiguousForBatch(): PcmFrame? {
        if (!playoutStarted) {
            return null
        }
        val expected = expectedSequence ?: return null
        val frame = frames.remove(expected) ?: return null
        expectedSequence = nextSeq(expected)
        stats.bufferedFrames = frames.size
        return frame
    }

    @Synchronized
    fun bufferedMs(): Int = frames.size * frameDurationMs

    // Drop oldest frames to quickly pull latency back toward target.
    @Synchronized
    fun dropOldestMs(ms: Int): Int {
        if (ms <= 0 || frames.isEmpty()) {
            return 0
        }
        val framesToDrop = kotlin.math.ceil(ms / frameDurationMs.toDouble()).toInt().coerceAtLeast(1)
        var dropped = 0
        repeat(framesToDrop) {
            if (frames.pollFirstEntry() != null) {
                dropped += 1
            }
        }
        if (dropped > 0) {
            stats.droppedFrames += dropped
            stats.bufferedFrames = frames.size
            val expected = expectedSequence
            if (expected != null) {
                val oldest = frames.firstEntry()?.key
                if (oldest == null || isOlder(expected, oldest)) {
                    expectedSequence = oldest
                    if (oldest == null) {
                        playoutStarted = false
                    }
                }
            }
        }
        return dropped
    }

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
        if (frames.size <= thresholdFrames) {
            overflowSinceMs = 0
            return
        }
        val now = SystemClock.elapsedRealtime()
        if (overflowSinceMs == 0L) {
            overflowSinceMs = now
            return
        }
        if (now - overflowSinceMs < TRIM_OBSERVE_WINDOW_MS) {
            return
        }
        while (frames.size > thresholdFrames) {
            frames.pollFirstEntry()
            stats.droppedFrames += 1
        }
        overflowSinceMs = 0
        val oldest = frames.firstEntry()?.key
        val expected = expectedSequence
        if (oldest == null) {
            playoutStarted = false
            expectedSequence = null
            return
        }
        if (expected == null || isOlder(expected, oldest)) {
            expectedSequence = oldest
        }
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

    private companion object {
        private const val HARD_FLOOR_MS = 8
        private const val TRIM_OBSERVE_WINDOW_MS = 250L
    }
}
