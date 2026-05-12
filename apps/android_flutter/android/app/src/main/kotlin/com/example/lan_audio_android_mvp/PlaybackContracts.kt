package com.example.lan_audio_android_mvp

object PlaybackActions {
    const val ACTION_START = "lan_audio.action.START"
    const val ACTION_PLAY_PAUSE = "lan_audio.action.PLAY_PAUSE"
    const val ACTION_STOP = "lan_audio.action.STOP"
    const val ACTION_RECONNECT = "lan_audio.action.RECONNECT"
    const val ACTION_RESTORE_LAST = "lan_audio.action.RESTORE_LAST"
    const val ACTION_SET_OPTIONS = "lan_audio.action.SET_OPTIONS"
    const val ACTION_SET_AUDIO_MODE = "lan_audio.action.SET_AUDIO_MODE"
    const val ACTION_SET_EQ = "lan_audio.action.SET_EQ"
    const val ACTION_SET_LOUDNESS = "lan_audio.action.SET_LOUDNESS"
    const val ACTION_DUMP_METRICS = "lan_audio.action.DUMP_METRICS"
    const val ACTION_START_MIC = "lan_audio.action.START_MIC"
    const val ACTION_STOP_MIC = "lan_audio.action.STOP_MIC"

    const val EXTRA_HOST = "host"
    const val EXTRA_WS_PORT = "ws_port"
    const val EXTRA_UDP_PORT = "udp_port"
    const val EXTRA_SERVER_NAME = "server_name"
    const val EXTRA_START_BUFFER_MS = "start_buffer_ms"
    const val EXTRA_MAX_BUFFER_MS = "max_buffer_ms"
    const val EXTRA_PING_INTERVAL_MS = "ping_interval_ms"
    const val EXTRA_AUDIO_MODE = "audio_mode"
    const val EXTRA_REASON = "reason"
    const val EXTRA_TRANSPORT_MODE = "transport_mode"
    const val EXTRA_OPEN_POWER_GUIDE = "open_power_guide"
    const val EXTRA_EQ_ENABLED = "eq_enabled"
    const val EXTRA_EQ_LOW_DB = "eq_low_db"
    const val EXTRA_EQ_MID_DB = "eq_mid_db"
    const val EXTRA_EQ_HIGH_DB = "eq_high_db"
    const val EXTRA_LOUDNESS_ENABLED = "loudness_enabled"
    const val EXTRA_MIC_HOST = "mic_host"
    const val EXTRA_REVERSE_PORT = "reverse_port"
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
    val transportMode: String = "wifi",
)

data class PlaybackOptions(
    val startBufferMs: Int = 80,
    val maxBufferMs: Int = 180,
    val batchFrames: Int = 2,
    val dropThresholdMs: Int = 160,
    // Total latency budget across jitter buffer + AudioTrack queue.
    val targetTotalLatencyMs: Int = 120,
    val maxTotalLatencyMs: Int = 150,
    val audioQueueSoftCapMs: Int = 90,
    val bufferingEnterDelayMs: Int = 180,
    val preferLowLatencyPath: Boolean = false,
    val preferStableAudioTrack: Boolean = true,
    val preferredCodec: String = "pcm16",
    val preferredSampleFormat: String = "pcm16",
    val lowLatencyBufferMultiplier: Int = 2,
    val lowLatencyFallbackBufferMultiplier: Int = 4,
    val frameDurationMs: Int = 10,
    val resetBufferOnSwitch: Boolean = true,
    val pingIntervalMs: Int = 1000,
)

data class PlaybackEqSettings(
    val enabled: Boolean = false,
    val lowDb: Int = 0,
    val midDb: Int = 0,
    val highDb: Int = 0,
) {
    fun clamped(): PlaybackEqSettings {
        return copy(
            lowDb = lowDb.coerceIn(MIN_GAIN_DB, MAX_GAIN_DB),
            midDb = midDb.coerceIn(MIN_GAIN_DB, MAX_GAIN_DB),
            highDb = highDb.coerceIn(MIN_GAIN_DB, MAX_GAIN_DB),
        )
    }

    companion object {
        const val MIN_GAIN_DB = -10
        const val MAX_GAIN_DB = 10
    }
}

data class LoudnessNormalizationSettings(
    val enabled: Boolean = false,
)

enum class TransportHint {
    Wifi,
    Usb,
}

data class PlaybackModeProfile(
    val mode: String,
    val transportHint: TransportHint,
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
    val lowLatencyBufferMultiplier: Int,
    val lowLatencyFallbackBufferMultiplier: Int,
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
            lowLatencyBufferMultiplier = lowLatencyBufferMultiplier,
            lowLatencyFallbackBufferMultiplier = lowLatencyFallbackBufferMultiplier,
            frameDurationMs = frameDurationMs,
            resetBufferOnSwitch = resetBufferOnSwitch,
            pingIntervalMs = pingIntervalMs,
        )
    }
}

object PlaybackModeProfiles {
    fun forMode(mode: String, transportHint: TransportHint = TransportHint.Wifi): PlaybackModeProfile {
        val normalized = normalize(mode)
        val base = when (normalized) {
            "low_latency" -> PlaybackModeProfile(
                mode = "low_latency",
                transportHint = transportHint,
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
                lowLatencyBufferMultiplier = 2,
                lowLatencyFallbackBufferMultiplier = 4,
                frameDurationMs = 10,
                resetBufferOnSwitch = true,
            )
            "high_quality" -> PlaybackModeProfile(
                mode = "high_quality",
                transportHint = transportHint,
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
                lowLatencyBufferMultiplier = 2,
                lowLatencyFallbackBufferMultiplier = 4,
                frameDurationMs = 10,
                resetBufferOnSwitch = false,
            )
            else -> PlaybackModeProfile(
                mode = "balanced",
                transportHint = transportHint,
                startBufferMs = 80,
                maxBufferMs = 180,
                batchFrames = 2,
                dropThresholdMs = 160,
                targetTotalLatencyMs = 120,
                maxTotalLatencyMs = 150,
                audioQueueSoftCapMs = 90,
                bufferingEnterDelayMs = 180,
                preferLowLatencyPath = false,
                preferStableAudioTrack = true,
                preferredCodec = "pcm16",
                preferredSampleFormat = "pcm16",
                lowLatencyBufferMultiplier = 2,
                lowLatencyFallbackBufferMultiplier = 4,
                frameDurationMs = 10,
                resetBufferOnSwitch = true,
            )
        }
        return applyTransportOverrides(base)
    }

    private fun applyTransportOverrides(base: PlaybackModeProfile): PlaybackModeProfile {
        return when (base.transportHint) {
            TransportHint.Usb -> when (base.mode) {
                "low_latency" -> base.copy(
                    startBufferMs = 100,
                    maxBufferMs = 200,
                    batchFrames = 1,
                    dropThresholdMs = 180,
                    targetTotalLatencyMs = 140,
                    maxTotalLatencyMs = 200,
                )
                "balanced" -> base.copy(
                    startBufferMs = 160,
                    maxBufferMs = 320,
                    batchFrames = 1,
                    dropThresholdMs = 280,
                    targetTotalLatencyMs = 220,
                    maxTotalLatencyMs = 320,
                )
                else -> base
            }
            TransportHint.Wifi -> when (base.mode) {
                "low_latency" -> base.copy(
                    startBufferMs = 20,
                    maxBufferMs = 50,
                    batchFrames = 1,
                    dropThresholdMs = 35,
                )
                else -> base
            }
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
    val totalBufferedMs: Int = 0,
    val jitterBufferedMs: Int = 0,
    val audioTrackQueuedMs: Int = 0,
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
    val startupSilenceFillCount: Int = 0,
    val rxFramesPerSec: Double = 0.0,
    val audioTrackWriteFramesPerSec: Double = 0.0,
    val cfgChangedCount: Int = 0,
    val discontinuityCount: Int = 0,
    val tcpRoundTripMs: Int? = null,
    val tcpRoundTripMedianMs: Int? = null,
    val jitterP95Ms: Int? = null,
    val floorHoldCount: Int = 0,
    val reconnectCount: Int = 0,
    val decodeErrors: Int = 0,
    val sinkWriteGapMsP95: Int = 0,
    val loudnessGainDb: Double = 0.0,
    val jitterHistoryUs: List<Int> = emptyList(),
    val jitterP50Us: Int = 0,
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
    val modeProfile: PlaybackModeProfile = PlaybackModeProfiles.forMode(currentAudioMode, TransportHint.Wifi),
    val connectionPath: String = "lan_ip_wifi_or_usb",
    val transportMode: String = "wifi",
    val playbackBackend: String = "audiotrack_stable",
    val connectedClientCount: Int = 0,
    val protocolPath: String = "legacy_or_v2_auto",
    val experimentalPath: Boolean = false,
    val effectiveCodec: String = "pcm16",
    val clientPlatform: String = "android",
    val clientAppVersion: String = "android_flutter",
    val serverPlatform: String? = null,
    val serverAppVersion: String? = null,
    val eqSettings: PlaybackEqSettings = PlaybackEqSettings(),
    val loudnessNormalizationEnabled: Boolean = false,
    val reconnectAttempts: Int = 0,
    val reconnectDelayMs: Int = 0,
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
                "transportHint" to modeProfile.transportHint.name.lowercase(),
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
                "lowLatencyBufferMultiplier" to modeProfile.lowLatencyBufferMultiplier,
                "lowLatencyFallbackBufferMultiplier" to modeProfile.lowLatencyFallbackBufferMultiplier,
                "frameDurationMs" to modeProfile.frameDurationMs,
                "resetBufferOnSwitch" to modeProfile.resetBufferOnSwitch,
            ),
            "connectionPath" to connectionPath,
            "transportMode" to transportMode,
            "playbackBackend" to playbackBackend,
            "connectedClientCount" to connectedClientCount,
            "protocolPath" to protocolPath,
            "experimentalPath" to experimentalPath,
            "effectiveCodec" to effectiveCodec,
            "clientPlatform" to clientPlatform,
            "clientAppVersion" to clientAppVersion,
            "serverPlatform" to serverPlatform,
            "serverAppVersion" to serverAppVersion,
            "eqEnabled" to eqSettings.enabled,
            "eqSettings" to mapOf(
                "enabled" to eqSettings.enabled,
                "lowDb" to eqSettings.lowDb,
                "midDb" to eqSettings.midDb,
                "highDb" to eqSettings.highDb,
            ),
            "loudnessNormalizationEnabled" to loudnessNormalizationEnabled,
            "reconnectAttempts" to reconnectAttempts,
            "reconnectDelayMs" to reconnectDelayMs,
            "metrics" to mapOf(
                "sampleRate" to metrics.sampleRate,
                "channels" to metrics.channels,
                "totalBufferedMs" to metrics.totalBufferedMs,
                "jitterBufferedMs" to metrics.jitterBufferedMs,
                "audioTrackQueuedMs" to metrics.audioTrackQueuedMs,
                // Backward-compatible key for old UI readers.
                "bufferedMs" to metrics.totalBufferedMs,
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
                "startupSilenceFillCount" to metrics.startupSilenceFillCount,
                "rxFramesPerSec" to metrics.rxFramesPerSec,
                "audioTrackWriteFramesPerSec" to metrics.audioTrackWriteFramesPerSec,
                "cfgChangedCount" to metrics.cfgChangedCount,
                "discontinuityCount" to metrics.discontinuityCount,
                "tcpRoundTripMs" to metrics.tcpRoundTripMs,
                "tcpRoundTripMedianMs" to metrics.tcpRoundTripMedianMs,
                "jitterP95Ms" to metrics.jitterP95Ms,
                "floorHoldCount" to metrics.floorHoldCount,
                "reconnectCount" to metrics.reconnectCount,
                "decodeErrors" to metrics.decodeErrors,
                "sinkWriteGapMsP95" to metrics.sinkWriteGapMsP95,
                "loudnessGainDb" to metrics.loudnessGainDb,
                "jitterHistoryUs" to metrics.jitterHistoryUs,
                "jitterP50Us" to metrics.jitterP50Us,
            ),
            "recentLog" to recentLog,
            "error" to error,
        )
    }
}

data class StableServiceMetrics(
    val bufferedMs: Int = 0,
    val underrun: Int = 0,
    val latePackets: Int = 0,
    val droppedPackets: Int = 0,
    val rttMs: Int = 0,
    val reconnectCount: Int = 0,
    val decodeErrors: Int = 0,
    val sinkWriteGapMsP95: Int = 0,
    val sampleRate: Int = 48_000,
    val channels: Int = 2,
    val jitterBufferedMs: Int = 0,
    val audioTrackQueuedMs: Int = 0,
    val audioTrackLatencyMs: Int? = null,
    val udpPackets: Int = 0,
    val udpBytes: Int = 0,
    val lossEstimate: Int = 0,
    val lastSeq: Long? = null,
    val startupSilenceFillCount: Int = 0,
    val jitterP95Ms: Int? = null,
    val floorHoldCount: Int = 0,
    val loudnessGainDb: Double = 0.0,
    val jitterHistoryUs: List<Int> = emptyList(),
    val jitterP50Us: Int = 0,
)

data class StableServiceSnapshot(
    val transport: String = "wifi",
    val mode: String = "balanced",
    val dataPlane: String = "legacy_las1",
    val activeDataPlane: String = "legacy_las1",
    val rollbackAvailable: Boolean = true,
    val codec: String = "pcm16",
    val effectiveCodec: String = "pcm16",
    val state: String = "disconnected",
    val rollbackState: String = "main_path_active",
    val protocolVersion: Int? = null,
    val modeProfile: Map<String, Any?> = emptyMap(),
    val negotiatedCapabilities: Map<String, Boolean> = emptyMap(),
    val serverPlatform: String? = null,
    val serverAppVersion: String? = null,
    val transportMode: String = "wifi",
    val playbackBackend: String = "audiotrack_stable",
    val connectedClientCount: Int = 0,
    val eqEnabled: Boolean = false,
    val eqSettings: Map<String, Any?> = emptyMap(),
    val loudnessNormalizationEnabled: Boolean = false,
    val reconnectAttempts: Int = 0,
    val reconnectDelayMs: Int = 0,
    val metrics: StableServiceMetrics = StableServiceMetrics(),
) {
    fun toMap(): Map<String, Any?> {
        return mapOf(
            "transport" to transport,
            "mode" to mode,
            "data_plane" to dataPlane,
            "active_data_plane" to activeDataPlane,
            "rollback_available" to rollbackAvailable,
            "codec" to codec,
            "effective_codec" to effectiveCodec,
            "state" to state,
            "rollback_state" to rollbackState,
            "protocol_version" to protocolVersion,
            "mode_profile" to modeProfile,
            "negotiated_capabilities" to negotiatedCapabilities,
            "server_platform" to serverPlatform,
            "server_app_version" to serverAppVersion,
            "transport_mode" to transportMode,
            "playback_backend" to playbackBackend,
            "connected_client_count" to connectedClientCount,
            "eq_enabled" to eqEnabled,
            "eq_settings" to eqSettings,
            "loudness_normalization_enabled" to loudnessNormalizationEnabled,
            "reconnect_attempts" to reconnectAttempts,
            "reconnect_delay_ms" to reconnectDelayMs,
            "metrics" to mapOf(
                "buffered_ms" to metrics.bufferedMs,
                "underrun" to metrics.underrun,
                "late_packets" to metrics.latePackets,
                "dropped_packets" to metrics.droppedPackets,
                "rtt_ms" to metrics.rttMs,
                "reconnect_count" to metrics.reconnectCount,
                "decode_errors" to metrics.decodeErrors,
                "sink_write_gap_ms_p95" to metrics.sinkWriteGapMsP95,
                "sample_rate" to metrics.sampleRate,
                "channels" to metrics.channels,
                "jitter_buffered_ms" to metrics.jitterBufferedMs,
                "audio_track_queued_ms" to metrics.audioTrackQueuedMs,
                "audio_track_latency_ms" to metrics.audioTrackLatencyMs,
                "udp_packets" to metrics.udpPackets,
                "udp_bytes" to metrics.udpBytes,
                "loss_estimate" to metrics.lossEstimate,
                "last_seq" to metrics.lastSeq,
                "startup_silence_fill_count" to metrics.startupSilenceFillCount,
                "jitter_p95_ms" to metrics.jitterP95Ms,
                "floor_hold_count" to metrics.floorHoldCount,
                "loudness_gain_db" to metrics.loudnessGainDb,
                "jitter_history_us" to metrics.jitterHistoryUs,
                "jitter_p50_us" to metrics.jitterP50Us,
            ),
        )
    }
}

fun PlaybackSnapshot.toStableServiceSnapshot(): StableServiceSnapshot {
    val dataPlane = when (protocolPath.lowercase()) {
        "v2_header" -> "v2_header"
        else -> "legacy_las1"
    }
    val activeDataPlane = dataPlane
    val requestedCodec = modeProfile.preferredCodec.ifBlank { effectiveCodec.ifBlank { "pcm16" } }
    val normalizedEffectiveCodec = effectiveCodec.ifBlank { "pcm16" }
    val state = when {
        connectionState == "reconnecting" -> "recovering"
        recentLog.startsWith("set_audio_mode:") ||
            recentLog.startsWith("set_audio_mode_pending:") ||
            recentLog.startsWith("audio_mode_changed:") ||
            recentLog.startsWith("v2_audio_mode_changed:") -> "reconfiguring"
        playbackState == "playing" -> "streaming"
        connectionState == "connected" -> "negotiated"
        connectionState == "connecting" -> "handshaking"
        serviceState == "stopping" || serviceState == "error" || connectionState == "error" -> "closed"
        else -> "disconnected"
    }
    val rollbackState = if (dataPlane == "legacy_las1" && normalizedEffectiveCodec == "pcm16") {
        "forced_legacy_las1_pcm16"
    } else {
        "main_path_active"
    }
    val rollbackAvailable = rollbackState != "forced_legacy_las1_pcm16"

    return StableServiceSnapshot(
        transport = if (transportMode == "usb") "usb" else "wifi",
        mode = PlaybackModeProfiles.normalize(currentAudioMode),
        dataPlane = dataPlane,
        activeDataPlane = activeDataPlane,
        rollbackAvailable = rollbackAvailable,
        codec = requestedCodec,
        effectiveCodec = normalizedEffectiveCodec,
        state = state,
        rollbackState = rollbackState,
        protocolVersion = protocolVersion,
        modeProfile = mapOf(
            "mode" to modeProfile.mode,
            "transport_hint" to modeProfile.transportHint.name.lowercase(),
            "start_buffer_ms" to modeProfile.startBufferMs,
            "max_buffer_ms" to modeProfile.maxBufferMs,
            "batch_frames" to modeProfile.batchFrames,
            "drop_threshold_ms" to modeProfile.dropThresholdMs,
            "target_total_latency_ms" to modeProfile.targetTotalLatencyMs,
            "max_total_latency_ms" to modeProfile.maxTotalLatencyMs,
            "audio_queue_soft_cap_ms" to modeProfile.audioQueueSoftCapMs,
            "buffering_enter_delay_ms" to modeProfile.bufferingEnterDelayMs,
            "prefer_low_latency_path" to modeProfile.preferLowLatencyPath,
            "prefer_stable_audio_track" to modeProfile.preferStableAudioTrack,
            "preferred_codec" to modeProfile.preferredCodec,
            "preferred_sample_format" to modeProfile.preferredSampleFormat,
            "low_latency_buffer_multiplier" to modeProfile.lowLatencyBufferMultiplier,
            "low_latency_fallback_buffer_multiplier" to modeProfile.lowLatencyFallbackBufferMultiplier,
            "frame_duration_ms" to modeProfile.frameDurationMs,
            "reset_buffer_on_switch" to modeProfile.resetBufferOnSwitch,
        ),
        negotiatedCapabilities = negotiatedCapabilities,
        serverPlatform = serverPlatform,
        serverAppVersion = serverAppVersion,
        transportMode = transportMode,
        playbackBackend = playbackBackend,
        connectedClientCount = connectedClientCount,
        eqEnabled = eqSettings.enabled,
        eqSettings = mapOf(
            "enabled" to eqSettings.enabled,
            "low_db" to eqSettings.lowDb,
            "mid_db" to eqSettings.midDb,
            "high_db" to eqSettings.highDb,
        ),
        loudnessNormalizationEnabled = loudnessNormalizationEnabled,
        reconnectAttempts = reconnectAttempts,
        reconnectDelayMs = reconnectDelayMs,
        metrics = StableServiceMetrics(
            bufferedMs = metrics.totalBufferedMs,
            underrun = metrics.jitterUnderrun,
            latePackets = metrics.jitterLate,
            droppedPackets = metrics.jitterDropped,
            rttMs = metrics.tcpRoundTripMs ?: 0,
            reconnectCount = metrics.reconnectCount,
            decodeErrors = metrics.decodeErrors,
            sinkWriteGapMsP95 = metrics.sinkWriteGapMsP95,
            sampleRate = metrics.sampleRate,
            channels = metrics.channels,
            jitterBufferedMs = metrics.jitterBufferedMs,
            audioTrackQueuedMs = metrics.audioTrackQueuedMs,
            audioTrackLatencyMs = metrics.audioTrackLatencyMs,
            udpPackets = metrics.udpPackets,
            udpBytes = metrics.udpBytes,
            lossEstimate = metrics.lossEstimate,
            lastSeq = metrics.lastSeq,
            startupSilenceFillCount = metrics.startupSilenceFillCount,
            jitterP95Ms = metrics.jitterP95Ms,
            floorHoldCount = metrics.floorHoldCount,
            loudnessGainDb = metrics.loudnessGainDb,
            jitterHistoryUs = metrics.jitterHistoryUs,
            jitterP50Us = metrics.jitterP50Us,
        ),
    )
}
