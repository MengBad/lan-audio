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

    fun writePcm16(data: ByteArray, frames: Int)

    fun stop()

    fun release()

    fun stats(): PlaybackAudioSinkStats

    fun backendLabel(options: PlaybackOptions): String
}
