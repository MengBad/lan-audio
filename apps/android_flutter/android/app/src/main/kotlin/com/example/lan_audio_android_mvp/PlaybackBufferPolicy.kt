package com.example.lan_audio_android_mvp

internal object PlaybackBufferPolicy {
    fun optionsFor(
        mode: String,
        transportHint: TransportHint,
        pingIntervalMs: Int = 1000,
    ): PlaybackOptions {
        return PlaybackModeProfiles.forMode(mode, transportHint).toOptions(pingIntervalMs)
    }

    fun newJitterBuffer(options: PlaybackOptions): PlaybackJitterBuffer {
        return PlaybackJitterBuffer(
            options.startBufferMs,
            options.maxBufferMs,
            options.dropThresholdMs,
        )
    }

    fun modeProfileForOptions(
        currentMode: String,
        transportHint: TransportHint,
        options: PlaybackOptions,
    ): PlaybackModeProfile {
        return PlaybackModeProfiles.forMode(currentMode, transportHint).copy(
            startBufferMs = options.startBufferMs,
            maxBufferMs = options.maxBufferMs,
            batchFrames = options.batchFrames,
            dropThresholdMs = options.dropThresholdMs,
            targetTotalLatencyMs = options.targetTotalLatencyMs,
            maxTotalLatencyMs = options.maxTotalLatencyMs,
            audioQueueSoftCapMs = options.audioQueueSoftCapMs,
            bufferingEnterDelayMs = options.bufferingEnterDelayMs,
            preferLowLatencyPath = options.preferLowLatencyPath,
            preferStableAudioTrack = options.preferStableAudioTrack,
            preferredCodec = options.preferredCodec,
            preferredSampleFormat = options.preferredSampleFormat,
            lowLatencyBufferMultiplier = options.lowLatencyBufferMultiplier,
            lowLatencyFallbackBufferMultiplier = options.lowLatencyFallbackBufferMultiplier,
            frameDurationMs = options.frameDurationMs,
            resetBufferOnSwitch = options.resetBufferOnSwitch,
        )
    }
}
