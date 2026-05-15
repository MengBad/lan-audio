package com.example.lan_audio_android_mvp

import android.media.AudioTrack

data class PlaybackAudioSinkStats(
    val nativeQueuedFrames: Int,
    val nativeQueuedAudioFrames: Int,
    val audioTrackWriteFrames: Long,
    val audioTrackShortWriteCount: Long,
    val writeGapP95Ms: Int = 0,
    val reportedLatencyMs: Int? = null,
    val lastPcmPeak: Int = 0,
    val lastPcmRms: Double = 0.0,
    val lastPlayState: Int = AudioTrack.PLAYSTATE_STOPPED,
    val silenceFillTotal: Long = 0,
    val underrunTotal: Int = 0,
    val ringBufferLevelFrames: Int = 0,
)

interface PlaybackAudioSink {
    fun init(
        sampleRate: Int,
        channels: Int,
        frameSamplesPerChannel: Int,
        transportHint: TransportHint = TransportHint.Wifi,
        encoding: Int,
    )

    fun start()

    fun setQueueSoftCapFrames(maxQueuedFrames: Int)

    fun setEqSettings(settings: PlaybackEqSettings) {}

    fun writePcm16(data: ByteArray, frames: Int)

    /// Phase 6.4 Hi-Res passthrough. Pushes raw big-endian 24-bit signed
    /// integer samples directly into the sink. The sink is expected to
    /// have been initialized with `encoding = ENCODING_PCM_FLOAT`. The
    /// default no-op throws so the legacy [AudioTrackController] surfaces
    /// "I never expected PCM24" instead of silently swallowing audio.
    fun writePcm24Be(data: ByteArray, frames: Int) {
        throw UnsupportedOperationException("PCM24 passthrough not supported by this sink")
    }

    fun stop()

    fun release()

    fun stats(): PlaybackAudioSinkStats

    fun backendLabel(options: PlaybackOptions): String
}
