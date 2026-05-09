package com.example.lan_audio_android_mvp

internal object PlaybackPacingEngine {
    const val BALANCED_EXPECTED_RX_FRAMES_PER_SEC = 50.0
    const val BALANCED_RX_FRAMES_FLOOR = 48.0
    const val BALANCED_PACING_SLOWDOWN_MAX_MS = 4

    private const val BALANCED_AUDIO_QUEUE_LOW_WATERMARK_EXTRA_MS = 10
    private const val BALANCED_AUDIO_QUEUE_FILL_TARGET_EXTRA_MS = 10
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
}
