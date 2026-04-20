package com.example.lan_audio_android_mvp

object PlaybackActions {
    const val ACTION_START = "lan_audio.action.START"
    const val ACTION_STOP = "lan_audio.action.STOP"
    const val ACTION_RECONNECT = "lan_audio.action.RECONNECT"
    const val ACTION_RESTORE_LAST = "lan_audio.action.RESTORE_LAST"
    const val ACTION_SET_OPTIONS = "lan_audio.action.SET_OPTIONS"
    const val ACTION_SET_AUDIO_MODE = "lan_audio.action.SET_AUDIO_MODE"
    const val ACTION_DUMP_METRICS = "lan_audio.action.DUMP_METRICS"

    const val EXTRA_HOST = "host"
    const val EXTRA_WS_PORT = "ws_port"
    const val EXTRA_UDP_PORT = "udp_port"
    const val EXTRA_SERVER_NAME = "server_name"
    const val EXTRA_START_BUFFER_MS = "start_buffer_ms"
    const val EXTRA_MAX_BUFFER_MS = "max_buffer_ms"
    const val EXTRA_PING_INTERVAL_MS = "ping_interval_ms"
    const val EXTRA_AUDIO_MODE = "audio_mode"
    const val EXTRA_REASON = "reason"
}

object PlaybackChannels {
    const val METHOD_PLAYBACK_SERVICE = "lan_audio/playback_service"
    const val EVENT_PLAYBACK_EVENTS = "lan_audio/playback_events"
}

data class PlaybackTarget(
    val host: String,
    val wsPort: Int,
    val udpPort: Int,
    val serverName: String,
)

data class PlaybackOptions(
    val startBufferMs: Int = 60,
    val maxBufferMs: Int = 300,
    val batchFrames: Int = 2,
    val dropThresholdMs: Int = 220,
    // Total latency budget across jitter buffer + AudioTrack queue.
    val targetTotalLatencyMs: Int = 140,
    val maxTotalLatencyMs: Int = 220,
    val audioQueueSoftCapMs: Int = 100,
    val bufferingEnterDelayMs: Int = 220,
    val preferLowLatencyPath: Boolean = false,
    val preferStableAudioTrack: Boolean = true,
    val preferredCodec: String = "pcm16",
    val preferredSampleFormat: String = "pcm16",
    val frameDurationMs: Int = 10,
    val resetBufferOnSwitch: Boolean = true,
    val pingIntervalMs: Int = 1000,
)

data class PlaybackModeProfile(
    val mode: String,
    val startBufferMs: Int,
    val maxBufferMs: Int,
    val batchFrames: Int,
    val dropThresholdMs: Int,
    val targetTotalLatencyMs: Int,
    val maxTotalLatencyMs: Int,
    val audioQueueSoftCapMs: Int,
    val bufferingEnterDelayMs: Int,
    val preferLowLatencyPath: Boolean,
    val preferStableAudioTrack: Boolean,
    val preferredCodec: String,
    val preferredSampleFormat: String,
    val frameDurationMs: Int,
    val resetBufferOnSwitch: Boolean,
) {
    fun toOptions(pingIntervalMs: Int = 1000): PlaybackOptions {
        return PlaybackOptions(
            startBufferMs = startBufferMs,
            maxBufferMs = maxBufferMs,
            batchFrames = batchFrames,
            dropThresholdMs = dropThresholdMs,
            targetTotalLatencyMs = targetTotalLatencyMs,
            maxTotalLatencyMs = maxTotalLatencyMs,
            audioQueueSoftCapMs = audioQueueSoftCapMs,
            bufferingEnterDelayMs = bufferingEnterDelayMs,
            preferLowLatencyPath = preferLowLatencyPath,
            preferStableAudioTrack = preferStableAudioTrack,
            preferredCodec = preferredCodec,
            preferredSampleFormat = preferredSampleFormat,
            frameDurationMs = frameDurationMs,
            resetBufferOnSwitch = resetBufferOnSwitch,
            pingIntervalMs = pingIntervalMs,
        )
    }
}

object PlaybackModeProfiles {
    fun forMode(mode: String): PlaybackModeProfile {
        return when (normalize(mode)) {
            "low_latency" -> PlaybackModeProfile(
                mode = "low_latency",
                startBufferMs = 40,
                maxBufferMs = 180,
                batchFrames = 1,
                dropThresholdMs = 30,
                targetTotalLatencyMs = 70,
                maxTotalLatencyMs = 110,
                audioQueueSoftCapMs = 40,
                bufferingEnterDelayMs = 120,
                preferLowLatencyPath = true,
                preferStableAudioTrack = false,
                preferredCodec = "pcm16",
                preferredSampleFormat = "pcm16",
                frameDurationMs = 10,
                resetBufferOnSwitch = true,
            )
            "high_quality" -> PlaybackModeProfile(
                mode = "high_quality",
                startBufferMs = 120,
                maxBufferMs = 500,
                batchFrames = 3,
                dropThresholdMs = 420,
                targetTotalLatencyMs = 420,
                maxTotalLatencyMs = 500,
                audioQueueSoftCapMs = 240,
                bufferingEnterDelayMs = 360,
                preferLowLatencyPath = false,
                preferStableAudioTrack = true,
                preferredCodec = "pcm16",
                preferredSampleFormat = "pcm16",
                frameDurationMs = 10,
                resetBufferOnSwitch = false,
            )
            else -> PlaybackModeProfile(
                mode = "balanced",
                startBufferMs = 60,
                maxBufferMs = 300,
                batchFrames = 2,
                dropThresholdMs = 220,
                targetTotalLatencyMs = 220,
                maxTotalLatencyMs = 300,
                audioQueueSoftCapMs = 120,
                bufferingEnterDelayMs = 280,
                preferLowLatencyPath = false,
                preferStableAudioTrack = true,
                preferredCodec = "pcm16",
                preferredSampleFormat = "pcm16",
                frameDurationMs = 10,
                resetBufferOnSwitch = true,
            )
        }
    }

    fun normalize(mode: String): String {
        return when (mode.lowercase()) {
            "low_latency" -> "low_latency"
            "high_quality" -> "high_quality"
            else -> "balanced"
        }
    }
}

data class PlaybackMetrics(
    val sampleRate: Int = 48000,
    val channels: Int = 2,
    val bufferedMs: Int = 0,
    val audioTrackLatencyMs: Int? = null,
    val jitterUnderrun: Int = 0,
    val jitterDropped: Int = 0,
    val jitterLate: Int = 0,
    val udpPackets: Int = 0,
    val udpBytes: Int = 0,
    val lossEstimate: Int = 0,
    val lastSeq: Long? = null,
    val nativeQueuedFrames: Int = 0,
    val audioTrackWriteFrames: Long = 0,
    val audioTrackShortWriteCount: Long = 0,
    val silenceFillCount: Int = 0,
    val rxFramesPerSec: Double = 0.0,
    val audioTrackWriteFramesPerSec: Double = 0.0,
    val cfgChangedCount: Int = 0,
    val discontinuityCount: Int = 0,
)

data class PlaybackSnapshot(
    val serviceState: String = "idle",
    val connectionState: String = "idle",
    val playbackState: String = "stopped",
    val targetHost: String? = null,
    val targetName: String? = null,
    val protocolVersion: Int? = null,
    val currentAudioMode: String = "balanced",
    val negotiatedCapabilities: Map<String, Boolean> = emptyMap(),
    val modeProfile: PlaybackModeProfile = PlaybackModeProfiles.forMode(currentAudioMode),
    val connectionPath: String = "lan_ip_wifi_or_usb",
    val playbackBackend: String = "audiotrack_stable",
    val protocolPath: String = "legacy_or_v2_auto",
    val experimentalPath: Boolean = false,
    val effectiveCodec: String = "pcm16",
    val clientPlatform: String = "android",
    val clientAppVersion: String = "android_flutter",
    val serverPlatform: String? = null,
    val serverAppVersion: String? = null,
    val metrics: PlaybackMetrics = PlaybackMetrics(),
    val recentLog: String = "",
    val error: Map<String, String>? = null,
) {
    fun toMap(): Map<String, Any?> {
        return mapOf(
            "serviceState" to serviceState,
            "connectionState" to connectionState,
            "playbackState" to playbackState,
            "targetHost" to targetHost,
            "targetName" to targetName,
            "protocolVersion" to protocolVersion,
            "currentAudioMode" to currentAudioMode,
            "negotiatedCapabilities" to negotiatedCapabilities,
            "modeProfile" to mapOf(
                "mode" to modeProfile.mode,
                "startBufferMs" to modeProfile.startBufferMs,
                "maxBufferMs" to modeProfile.maxBufferMs,
                "batchFrames" to modeProfile.batchFrames,
                "dropThresholdMs" to modeProfile.dropThresholdMs,
                "targetTotalLatencyMs" to modeProfile.targetTotalLatencyMs,
                "maxTotalLatencyMs" to modeProfile.maxTotalLatencyMs,
                "audioQueueSoftCapMs" to modeProfile.audioQueueSoftCapMs,
                "bufferingEnterDelayMs" to modeProfile.bufferingEnterDelayMs,
                "preferLowLatencyPath" to modeProfile.preferLowLatencyPath,
                "preferStableAudioTrack" to modeProfile.preferStableAudioTrack,
                "preferredCodec" to modeProfile.preferredCodec,
                "preferredSampleFormat" to modeProfile.preferredSampleFormat,
                "frameDurationMs" to modeProfile.frameDurationMs,
                "resetBufferOnSwitch" to modeProfile.resetBufferOnSwitch,
            ),
            "connectionPath" to connectionPath,
            "playbackBackend" to playbackBackend,
            "protocolPath" to protocolPath,
            "experimentalPath" to experimentalPath,
            "effectiveCodec" to effectiveCodec,
            "clientPlatform" to clientPlatform,
            "clientAppVersion" to clientAppVersion,
            "serverPlatform" to serverPlatform,
            "serverAppVersion" to serverAppVersion,
            "metrics" to mapOf(
                "sampleRate" to metrics.sampleRate,
                "channels" to metrics.channels,
                "bufferedMs" to metrics.bufferedMs,
                "audioTrackLatencyMs" to metrics.audioTrackLatencyMs,
                "jitterUnderrun" to metrics.jitterUnderrun,
                "jitterDropped" to metrics.jitterDropped,
                "jitterLate" to metrics.jitterLate,
                "udpPackets" to metrics.udpPackets,
                "udpBytes" to metrics.udpBytes,
                "lossEstimate" to metrics.lossEstimate,
                "lastSeq" to metrics.lastSeq,
                "nativeQueuedFrames" to metrics.nativeQueuedFrames,
                "audioTrackWriteFrames" to metrics.audioTrackWriteFrames,
                "audioTrackShortWriteCount" to metrics.audioTrackShortWriteCount,
                "silenceFillCount" to metrics.silenceFillCount,
                "rxFramesPerSec" to metrics.rxFramesPerSec,
                "audioTrackWriteFramesPerSec" to metrics.audioTrackWriteFramesPerSec,
                "cfgChangedCount" to metrics.cfgChangedCount,
                "discontinuityCount" to metrics.discontinuityCount,
            ),
            "recentLog" to recentLog,
            "error" to error,
        )
    }
}
