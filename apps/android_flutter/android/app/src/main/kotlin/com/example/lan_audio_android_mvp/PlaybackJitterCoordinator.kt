package com.example.lan_audio_android_mvp

data class PlaybackJitterDecision(
    val shouldDrain: Boolean,
    val shouldRefillAudioQueue: Boolean,
    val writeBatchFrames: Int,
)

class PlaybackJitterCoordinator {
    fun decide(
        options: PlaybackOptions,
        jitterBufferedMs: Int,
        audioQueuedMs: Int,
    ): PlaybackJitterDecision {
        val frameMs = options.frameDurationMs.coerceAtLeast(1)
        val lowWatermarkMs = if (options.preferLowLatencyPath) {
            frameMs
        } else {
            frameMs * 2
        }
        val fillTargetMs = if (options.preferLowLatencyPath) {
            frameMs * options.batchFrames.coerceAtLeast(1)
        } else {
            frameMs * 5
        }
        val shouldDrain = jitterBufferedMs > options.dropThresholdMs
        val shouldRefillAudioQueue = jitterBufferedMs > 0 && audioQueuedMs <= lowWatermarkMs
        val batchFrames = when {
            shouldRefillAudioQueue -> (fillTargetMs / frameMs).coerceAtLeast(1)
            else -> options.batchFrames.coerceAtLeast(1)
        }
        return PlaybackJitterDecision(
            shouldDrain = shouldDrain,
            shouldRefillAudioQueue = shouldRefillAudioQueue,
            writeBatchFrames = batchFrames,
        )
    }
}
