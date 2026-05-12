package com.example.lan_audio_android_mvp

internal object PlaybackPacingEngine {
    const val BALANCED_EXPECTED_RX_FRAMES_PER_SEC = 50.0
    const val BALANCED_RX_FRAMES_FLOOR = 48.0
    const val BALANCED_PACING_SLOWDOWN_MAX_MS = 4

    private const val BALANCED_AUDIO_QUEUE_LOW_WATERMARK_EXTRA_MS = 10
    private const val BALANCED_AUDIO_QUEUE_FILL_TARGET_EXTRA_MS = 10
    private const val BALANCED_ONE_SHOT_REFILL_TARGET_MS = 50
    private const val BUFFER_EMPTY_LOW_WATERMARK_MS = 20

    fun pacingOffsetMs(
        options: PlaybackOptions,
        totalBufferedMs: Int,
        catchupMs: Int,
        frameDurationMs: Int,
        audioQueuedMs: Int,
        smoothedRxFramesPerSec: Double?,
    ): Int {
        if (options.preferLowLatencyPath) {
            return when {
                catchupMs >= 120 -> -4
                catchupMs >= 70 -> -3
                catchupMs >= 35 -> -2
                else -> 0
            }
        }

        val lowerBoundMs = options.startBufferMs.coerceAtLeast(options.targetTotalLatencyMs - 40)
        val upperBoundMs = options.maxTotalLatencyMs.coerceAtMost(options.targetTotalLatencyMs + 30)
        val rxBelowSteadyState = (smoothedRxFramesPerSec ?: BALANCED_EXPECTED_RX_FRAMES_PER_SEC) <
            BALANCED_RX_FRAMES_FLOOR
        val audioQueueLowWatermarkMs = balancedAudioQueueLowWatermarkMs(frameDurationMs)
        return when {
            audioQueuedMs <= audioQueueLowWatermarkMs / 2 -> -2
            audioQueuedMs < audioQueueLowWatermarkMs -> -1
            totalBufferedMs <= lowerBoundMs - frameDurationMs -> 2
            totalBufferedMs < lowerBoundMs -> 1
            rxBelowSteadyState -> 0
            totalBufferedMs >= upperBoundMs + frameDurationMs -> -1
            else -> 0
        }
    }

    fun balancedAudioQueueLowWatermarkMs(frameDurationMs: Int): Int {
        return (frameDurationMs * 2 + BALANCED_AUDIO_QUEUE_LOW_WATERMARK_EXTRA_MS)
            .coerceAtLeast(BUFFER_EMPTY_LOW_WATERMARK_MS)
    }

    fun balancedAudioQueueFillTargetMs(frameDurationMs: Int): Int {
        return balancedAudioQueueLowWatermarkMs(frameDurationMs) + BALANCED_AUDIO_QUEUE_FILL_TARGET_EXTRA_MS
    }

    fun targetWriteBatchSize(
        options: PlaybackOptions,
        currentMode: String,
        audioQueuedMs: Int,
        jitterBufferedMs: Int,
        frameDurationMs: Int,
        state: BalancedRefillState,
    ): Int {
        val baseBatchSize = options.batchFrames.coerceIn(1, 4)
        if (options.preferLowLatencyPath || currentMode != "balanced") {
            state.reset()
            return baseBatchSize
        }

        val fillTargetMs = balancedAudioQueueFillTargetMs(frameDurationMs)
        if (audioQueuedMs >= fillTargetMs) {
            state.reset()
            return baseBatchSize
        }

        val availableFrames = ((jitterBufferedMs / frameDurationMs).coerceAtLeast(0) + 1).coerceAtLeast(1)
        if (!state.queueBelowFillTarget) {
            state.queueBelowFillTarget = true
            val refillFramesNeeded = kotlin.math.ceil(
                (BALANCED_ONE_SHOT_REFILL_TARGET_MS - audioQueuedMs).coerceAtLeast(0) /
                    frameDurationMs.toDouble(),
            ).toInt().coerceAtLeast(1)
            return refillFramesNeeded.coerceAtMost(availableFrames)
        }

        return baseBatchSize.coerceAtMost(availableFrames)
    }
}

internal class BalancedRefillState {
    var queueBelowFillTarget: Boolean = false

    fun reset() {
        queueBelowFillTarget = false
    }
}
