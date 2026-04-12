package com.example.lan_audio_android_mvp

object PlaybackActions {
    const val ACTION_START = "lan_audio.action.START"
    const val ACTION_STOP = "lan_audio.action.STOP"
    const val ACTION_RECONNECT = "lan_audio.action.RECONNECT"
    const val ACTION_SET_OPTIONS = "lan_audio.action.SET_OPTIONS"

    const val EXTRA_HOST = "host"
    const val EXTRA_WS_PORT = "ws_port"
    const val EXTRA_UDP_PORT = "udp_port"
    const val EXTRA_SERVER_NAME = "server_name"
    const val EXTRA_START_BUFFER_MS = "start_buffer_ms"
    const val EXTRA_MAX_BUFFER_MS = "max_buffer_ms"
    const val EXTRA_PING_INTERVAL_MS = "ping_interval_ms"
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
    val pingIntervalMs: Int = 1000,
)

data class PlaybackMetrics(
    val sampleRate: Int = 48000,
    val channels: Int = 2,
    val bufferedMs: Int = 0,
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
)

data class PlaybackSnapshot(
    val serviceState: String = "idle",
    val connectionState: String = "idle",
    val playbackState: String = "stopped",
    val targetHost: String? = null,
    val targetName: String? = null,
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
            "metrics" to mapOf(
                "sampleRate" to metrics.sampleRate,
                "channels" to metrics.channels,
                "bufferedMs" to metrics.bufferedMs,
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
            ),
            "recentLog" to recentLog,
            "error" to error,
        )
    }
}
