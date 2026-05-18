package com.example.lan_audio_android_mvp

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.util.Locale
import java.util.ArrayDeque
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.ThreadFactory
import java.util.concurrent.TimeUnit

class PlaybackSessionRuntime(
    private val context: Context,
    private val stateStore: PlaybackStateStore,
) {
    private val logTag = "lan_audio_session"
    private val decodeLogTag = "lan_audio_decode"
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private var playbackSink: PlaybackAudioSink = createPlaybackSink()
    private val opusDecoder = OpusFrameDecoder()
    private val controlExecutor: ScheduledExecutorService =
        Executors.newSingleThreadScheduledExecutor(playoutThreadFactory())
    private var playoutFuture: ScheduledFuture<*>? = null
    private var reconnectFuture: ScheduledFuture<*>? = null
    private var reconnectAttemptCount = 0
    private var reconnectTotalCount = 0
    private var streamManager: StreamSessionManager? = null
    private var options = PlaybackModeProfiles.forMode("balanced").toOptions()
    private var eqSettings = PlaybackEqSettings()
    private var loudnessNormalizationEnabled = false
    private val loudnessNormalizer = PlaybackLoudnessNormalizer()
    private var jitterBuffer = newJitterBuffer(options)
    private var currentTransportHint: TransportHint = TransportHint.Wifi
    private var currentTarget: PlaybackTarget? = null
    private var focusRequest: AudioFocusRequest? = null
    private var legacyFocusListener: AudioManager.OnAudioFocusChangeListener? = null
    private var audioFocusAcquired = false
    private var noisyRegistered = false
    private var audioStarted = false
    private var audioInit = false
    private var lastSeq: Long? = null
    private var udpLoss: Int = 0
    private var streamGeneration: Long = 0
    private var silenceFillCount: Int = 0
    private var sinkSilenceFillTotal: Int = 0
    private var startupSilenceFillCount: Int = 0
    private var startupSilenceFillBaseline: Int = 0
    private var startupSilenceTrackingActive: Boolean = false
    private var startupSilencePhaseUntilMs: Long = 0
    private var cfgChangedCount: Int = 0
    private var discontinuityCount: Int = 0
    private var lastMetricsLogAtMs: Long = 0
    private var lastLoggedUdpPackets: Int = 0
    private var lastLoggedAudioTrackWriteFrames: Long = 0
    private var currentPacketCodecLabel: String = "pcm16"
    private var nextPlayoutAtMs: Long = 0
    private var consecutivePlayoutMisses: Int = 0
    private var consecutiveEmptyQueueMisses: Int = 0
    private var consecutivePlayoutHits: Int = 0
    private var bufferingCandidateSinceMs: Long = 0
    private var playbackStateStableSinceMs: Long = 0
    private var lastLatencyGuardAtMs: Long = 0
    private var latencyGuardOverflowSinceMs: Long = 0
    private var latencyGuardDropCount: Int = 0
    private var lastPacketArrivalAtMs: Long = 0
    private val recentArrivalIntervalsMs = ArrayDeque<Int>()
    private var jitterP95Ms: Int? = null
    private var adaptiveStartBufferMs: Int? = null
    private val jitterHistoryUs = IntArray(120)
    private var jitterHistoryIdx = 0
    private var lastPacketArrivalUs = 0L
    private var expectedArrivalUs = 0L
    private var jitterP50Us = 0
    private var adaptiveStableSinceMs: Long = 0
    private var lastDecodeSummaryAtMs: Long = 0
    private var lastRxSeqRaw: Int? = null
    private var rxSeqGapCount = 0
    private var rxLastSeqWindow: Int? = null
    private var decodeWindowMsSamples = ArrayList<Float>()
    private var decodeProducedWindowFrames = 0
    private var decodeFailCount = 0
    private var decodeErrorTotal = 0
    private var rxWindowFrames = 0
    private var lastAudioFormat: ActiveAudioFormat? = null
    private var oboeUnderrunCount = 0
    private var smoothedRxFramesPerSec: Double? = null
    private var lastDiagnosedSilenceFillCount: Int = 0
    private var lastDiagnosedSinkSilenceFillCount: Int = 0
    private var lastDiagnosedOboeUnderrunCount: Int = 0

    // Phase 3: track last-reported watermark counters so we can emit
    // monotonic deltas to the server's adaptive sync engine without
    // double-counting events the server already saw.
    private var lastReportedSinkSilenceFillCount: Int = 0
    private var lastReportedOboeUnderrunCount: Int = 0
    private var balancedQueueBelowFillTarget: Boolean = false
    @Volatile
    private var highQualityPrefillActive = false
    @Volatile
    private var highQualityPrefillDeadlineMs: Long = 0L
    @Volatile
    private var highQualityPrefillTargetMs: Int = 0
    private val playbackSinkLock = Any()
    private val modeSwitchLock = Any()
    @Volatile
    private var modeSwitchInFlight = false

    private val noisyReceiver = object : BroadcastReceiver() {
        override fun onReceive(ctx: Context?, intent: Intent?) {
            if (intent?.action == AudioManager.ACTION_AUDIO_BECOMING_NOISY) {
                stopPlayback("audio_becoming_noisy")
            }
        }
    }

    fun startPlayback(target: PlaybackTarget) {
        Log.i(logTag, "startPlayback target=${target.serverName} host=${target.host} ws=${target.wsPort} udp=${target.udpPort}")
        currentTarget = target
        currentTransportHint = transportHintFromWire(target.transportMode)
        val currentMode = PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode)
        options = PlaybackModeProfiles.forMode(currentMode, currentTransportHint).toOptions(options.pingIntervalMs)
        loudnessNormalizer.setMode(currentMode)
        reconnectFuture?.cancel(false)
        reconnectFuture = null
        reconnectAttemptCount = 0
        reconnectTotalCount = 0
        stopStreamAndAudio()
        val generation = ++streamGeneration

        if (!requestAudioFocus()) {
            Log.w(logTag, "audio focus denied")
            publishError("audio_focus_denied", "audio focus denied")
            return
        }
        registerNoisyReceiver()

        jitterBuffer = newJitterBuffer(options)
        lastSeq = null
        udpLoss = 0
        silenceFillCount = 0
        sinkSilenceFillTotal = 0
        startupSilenceFillCount = 0
        startupSilenceFillBaseline = 0
        startupSilenceTrackingActive = true
        startupSilencePhaseUntilMs = 0L
        cfgChangedCount = 0
        discontinuityCount = 0
        lastMetricsLogAtMs = 0
        lastLoggedUdpPackets = 0
        lastLoggedAudioTrackWriteFrames = 0
        currentPacketCodecLabel = "pcm16"
        nextPlayoutAtMs = 0
        consecutivePlayoutMisses = 0
        consecutiveEmptyQueueMisses = 0
        consecutivePlayoutHits = 0
        bufferingCandidateSinceMs = 0
        playbackStateStableSinceMs = SystemClock.elapsedRealtime()
        lastLatencyGuardAtMs = 0
        latencyGuardOverflowSinceMs = 0
        latencyGuardDropCount = 0
        lastPacketArrivalAtMs = 0
        recentArrivalIntervalsMs.clear()
        jitterP95Ms = null
        adaptiveStartBufferMs = null
        adaptiveStableSinceMs = 0
        lastDecodeSummaryAtMs = 0
        lastRxSeqRaw = null
        rxSeqGapCount = 0
        rxLastSeqWindow = null
        decodeWindowMsSamples = ArrayList()
        decodeProducedWindowFrames = 0
        decodeFailCount = 0
        decodeErrorTotal = 0
        rxWindowFrames = 0
        lastAudioFormat = null
        oboeUnderrunCount = 0
        smoothedRxFramesPerSec = null
        lastDiagnosedSilenceFillCount = 0
        lastDiagnosedSinkSilenceFillCount = 0
        lastDiagnosedOboeUnderrunCount = 0
        lastReportedSinkSilenceFillCount = 0
        lastReportedOboeUnderrunCount = 0
        balancedQueueBelowFillTarget = false
        highQualityPrefillActive = false
        highQualityPrefillDeadlineMs = 0L
        highQualityPrefillTargetMs = 0
        withPlaybackSinkLock {
            playbackSink.setQueueSoftCapFrames(msToFrames(options.audioQueueSoftCapMs))
        }

        stateStore.update {
            it.copy(
            serviceState = "running",
            connectionState = "connecting",
            playbackState = "buffering",
            reconnectAttempts = 0,
            reconnectDelayMs = 0,
            modeProfile = PlaybackModeProfiles.forMode(it.currentAudioMode, currentTransportHint),
                connectionPath = if (target.transportMode == "usb") "usb_localhost" else "lan_ip_wifi_or_usb",
        playbackBackend = playbackBackendLabel(),
                protocolPath = "legacy_or_v2_auto",
                transportMode = target.transportMode,
                connectedClientCount = 0,
                experimentalPath = false,
                effectiveCodec = "pcm16",
                eqSettings = eqSettings,
                loudnessNormalizationEnabled = loudnessNormalizationEnabled,
                targetHost = target.host,
                targetName = target.serverName,
                recentLog = "connecting:${target.serverName}(${target.host})",
                error = null,
                metrics = it.metrics.copy(
                    totalBufferedMs = 0,
                    jitterBufferedMs = 0,
                    audioTrackQueuedMs = 0,
                    jitterUnderrun = 0,
                    jitterDropped = 0,
                    jitterLate = 0,
                    udpPackets = 0,
                    udpBytes = 0,
                    lossEstimate = 0,
                    lastSeq = null,
                    silenceFillCount = 0,
                    startupSilenceFillCount = 0,
                    rxFramesPerSec = 0.0,
                    audioTrackWriteFramesPerSec = 0.0,
                    cfgChangedCount = 0,
                    discontinuityCount = 0,
                    tcpRoundTripMs = null,
                    tcpRoundTripMedianMs = null,
                    jitterP95Ms = null,
                    floorHoldCount = 0,
                    reconnectCount = 0,
                    decodeErrors = 0,
                    sinkWriteGapMsP95 = 0,
                    loudnessGainDb = 0.0,
                ),
            )
        }

        streamManager = StreamSessionManager(
            target,
            options.pingIntervalMs,
            stateStore.current().currentAudioMode,
            callback = object :
            StreamSessionManager.Callback {
            override fun onLog(message: String) {
                if (generation != streamGeneration) return
                stateStore.update { it.copy(recentLog = message) }
            }

            override fun onWsConnected() {
                if (generation != streamGeneration) return
                Log.i(logTag, "ws connected")
                reconnectFuture?.cancel(false)
                reconnectFuture = null
                reconnectAttemptCount = 0
                stateStore.update {
                    it.copy(
                    connectionState = "connected",
                    playbackState = "buffering",
                    reconnectAttempts = 0,
                    reconnectDelayMs = 0,
                    connectedClientCount = if (it.connectedClientCount <= 0) 1 else it.connectedClientCount,
                    recentLog = "ws_connected",
                    error = null,
                )
                }
                ensurePlayoutLoop()
            }

            override fun onWsDisconnected(reason: String) {
                if (generation != streamGeneration) return
                Log.w(logTag, "ws disconnected: $reason")
                if (currentTarget == null) {
                    return
                }
                stateStore.update {
                    it.copy(
                        connectionState = "reconnecting",
                        playbackState = "buffering",
                        recentLog = reason,
                    )
                }
                scheduleReconnect(reason)
            }

            override fun onUdpPacket(packet: LasPacket) {
                if (generation != streamGeneration) return
                handleUdpPacket(packet)
            }

            override fun onControlHelloAck(
                protocolVersion: Int,
                currentAudioMode: String,
                capabilities: Map<String, Boolean>,
                transportType: String,
            ) {
                if (generation != streamGeneration) return
                applyAudioModeProfile(currentAudioMode, "hello_ack", transportType)
                stateStore.update {
                    it.copy(
                        protocolVersion = protocolVersion,
                        currentAudioMode = PlaybackModeProfiles.normalize(currentAudioMode),
                        modeProfile = PlaybackModeProfiles.forMode(
                            currentAudioMode,
                            currentTransportHint,
                        ),
                        transportMode = if (currentTransportHint == TransportHint.Usb) "usb" else "wifi",
                        negotiatedCapabilities = capabilities,
                        recentLog = "hello_ack_v2",
                    )
                }
            }

            override fun onServerInfo(platform: String?, appVersion: String?, currentAudioMode: String) {
                if (generation != streamGeneration) return
                applyAudioModeProfile(currentAudioMode, "server_info")
                stateStore.update {
                    it.copy(
                        serverPlatform = platform,
                        serverAppVersion = appVersion,
                        currentAudioMode = PlaybackModeProfiles.normalize(currentAudioMode),
                        modeProfile = PlaybackModeProfiles.forMode(
                            currentAudioMode,
                            currentTransportHint,
                        ),
                    )
                }
            }

            override fun onAudioModeChanged(mode: String, applied: Boolean, reason: String) {
                if (generation != streamGeneration) return
                if (applied) {
                    applyAudioModeProfile(mode, reason)
                }
                stateStore.update {
                    it.copy(
                        currentAudioMode = PlaybackModeProfiles.normalize(mode),
                        modeProfile = PlaybackModeProfiles.forMode(mode, currentTransportHint),
                        recentLog = if (applied) "audio_mode_changed:$mode" else "audio_mode_rejected:$reason",
                    )
                }
            }

            override fun onClientCountUpdated(count: Int) {
                if (generation != streamGeneration) return
                stateStore.update {
                    it.copy(connectedClientCount = count.coerceAtLeast(0))
                }
            }

            override fun onTcpRoundTripMs(roundTripMs: Int, medianMs: Int) {
                if (generation != streamGeneration) return
                stateStore.update {
                    it.copy(
                        metrics = it.metrics.copy(
                            tcpRoundTripMs = roundTripMs,
                            tcpRoundTripMedianMs = medianMs,
                        ),
                    )
                }
            }

            override fun onError(code: String, message: String) {
                if (generation != streamGeneration) return
                Log.e(logTag, "stream error code=$code message=$message")
                publishError(code, message)
                scheduleReconnect(code)
            }

            override fun provideWatermark(): StreamSessionManager.WatermarkSample? {
                if (generation != streamGeneration) return null
                return try {
                    buildWatermarkSample()
                } catch (t: Throwable) {
                    Log.w(logTag, "buildWatermarkSample failed: ${t.message}")
                    null
                }
            }
        })
        streamManager?.start()
    }

    fun stopPlayback(reason: String = "user_stop") {
        Log.i(logTag, "stopPlayback reason=$reason")
        reconnectFuture?.cancel(false)
        reconnectFuture = null
        reconnectAttemptCount = 0
        reconnectTotalCount = 0
        stateStore.update {
            it.copy(
            serviceState = "stopping",
            connectionState = "disconnected",
            playbackState = "stopped",
            reconnectAttempts = 0,
            reconnectDelayMs = 0,
            recentLog = reason,
            )
        }
        stopStreamAndAudio()
        unregisterNoisyReceiver()
        releaseAudioFocus()
        stateStore.update {
            it.copy(
            serviceState = "idle",
            connectionState = "idle",
            playbackState = "stopped",
            reconnectAttempts = 0,
            reconnectDelayMs = 0,
            recentLog = reason,
            )
        }
    }

    fun reconnect(reason: String = "manual_reconnect") {
        Log.i(logTag, "reconnect reason=$reason")
        scheduleReconnect(reason, immediate = true)
    }

    fun hasActiveTarget(): Boolean {
        return currentTarget != null
    }

    fun setOptions(next: PlaybackOptions) {
        val snapshot = stateStore.current()
        Log.i(
            logTag,
            "setOptions startBufferMs=${next.startBufferMs} maxBufferMs=${next.maxBufferMs} pingIntervalMs=${next.pingIntervalMs} serviceState=${snapshot.serviceState} playbackState=${snapshot.playbackState}"
        )
        options = next
        jitterBuffer.reconfigure(options.startBufferMs, options.maxBufferMs, options.dropThresholdMs)
        withPlaybackSinkLock {
            playbackSink.setQueueSoftCapFrames(msToFrames(next.audioQueueSoftCapMs))
        }
        stateStore.update {
            it.copy(
                modeProfile = modeProfileForOptions(
                    currentMode = it.currentAudioMode,
                    transportHint = currentTransportHint,
                    options = options,
                ),
                playbackBackend = playbackBackendLabel(),
                recentLog = "options_updated",
            )
        }
    }

    fun setAudioMode(
        mode: String,
        reason: String = "user_selected",
        preferredCodec: String? = null,
    ) {
        val normalized = PlaybackModeProfiles.normalize(mode)
        val hasActiveStream = streamManager != null
        val sent = streamManager?.setAudioMode(normalized, reason, preferredCodec) ?: false
        if (!hasActiveStream || !sent) {
            applyAudioModeProfile(
                normalized,
                if (sent) reason else "$reason:local_apply",
            )
        }
        stateStore.update {
            if (sent && hasActiveStream) {
                it.copy(
                    playbackBackend = playbackBackendLabel(),
                    recentLog = "set_audio_mode_pending_ack:$normalized",
                )
            } else {
                it.copy(
                    currentAudioMode = normalized,
                    modeProfile = PlaybackModeProfiles.forMode(normalized, currentTransportHint),
                    playbackBackend = playbackBackendLabel(),
                    recentLog = if (hasActiveStream) {
                        "set_audio_mode_send_failed_local:$normalized"
                    } else {
                        "set_audio_mode_local:$normalized"
                    },
                )
            }
        }
    }

    fun setEqSettings(settings: PlaybackEqSettings) {
        val clamped = settings.clamped()
        eqSettings = clamped
        withPlaybackSinkLock {
            playbackSink.setEqSettings(clamped)
        }
        stateStore.update {
            it.copy(
                eqSettings = clamped,
                recentLog = "eq_updated",
            )
        }
    }

    fun setLoudnessNormalization(enabled: Boolean) {
        loudnessNormalizationEnabled = enabled
        loudnessNormalizer.setEnabled(enabled)
        loudnessNormalizer.setMode(stateStore.current().currentAudioMode)
        stateStore.update {
            it.copy(
                loudnessNormalizationEnabled = enabled,
                metrics = it.metrics.copy(loudnessGainDb = loudnessNormalizer.gainDb()),
                recentLog = if (enabled) "loudness_normalization_on" else "loudness_normalization_off",
            )
        }
    }

    fun dumpMetrics(reason: String = "manual_request") {
        maybeLogMetrics(force = true, reason = reason)
    }

    fun destroy() {
        reconnectFuture?.cancel(true)
        controlExecutor.shutdownNow()
        stopPlayback("controller_destroy")
    }

    private fun playoutThreadFactory(): ThreadFactory {
        return ThreadFactory { runnable ->
            Thread({
                val threadName = Thread.currentThread().name
                val threadId = Process.myTid()
                try {
                    Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
                    val effectivePriority = Process.getThreadPriority(threadId)
                    Log.i(
                        logTag,
                        "playout executor thread priority set name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) effective=$effectivePriority",
                    )
                } catch (t: Throwable) {
                    Log.w(
                        logTag,
                        "playout executor thread priority set failed name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) error=${t.message}",
                    )
                }
                runnable.run()
            }, "lan-audio-service-playout")
        }
    }

    private fun ensurePlayoutLoop() {
        if (playoutFuture != null && !playoutFuture!!.isCancelled) {
            return
        }
        playoutFuture = controlExecutor.scheduleAtFixedRate({
            try {
                playoutTick()
            } catch (t: Throwable) {
                Log.e(logTag, "playout tick failed", t)
                publishError("playout_tick_failed", t.message ?: "playout failed")
            }
        }, 10L, 10L, TimeUnit.MILLISECONDS)
    }

    private fun handleUdpPacket(packet: LasPacket) {
        rxWindowFrames += 1
        val currentSeq = packet.sequence
        rxLastSeqWindow = currentSeq

        // Jitter timing: deviation from expected inter-packet arrival
        val nowUs = System.nanoTime() / 1000L
        if (expectedArrivalUs == 0L) {
            expectedArrivalUs = nowUs
        } else {
            val frameDurationUs = options.frameDurationMs * 1000L
            expectedArrivalUs += frameDurationUs
            val jitterUs = Math.abs(nowUs - expectedArrivalUs).toInt()
            jitterHistoryUs[jitterHistoryIdx % 120] = jitterUs
            jitterHistoryIdx++
            // Re-align expectation if we drifted too far
            if (jitterUs > frameDurationUs * 8) {
                expectedArrivalUs = nowUs
            }
        }
        lastPacketArrivalUs = nowUs

        val previousSeq = lastRxSeqRaw
        if (previousSeq != null) {
            val expectedRawSeq = ((previousSeq.toLong() + 1L) and 0xFFFFFFFFL).toInt()
            if (currentSeq != expectedRawSeq) {
                rxSeqGapCount += 1
            }
        }
        lastRxSeqRaw = currentSeq
        if (packet.hasDiscontinuity) {
            discontinuityCount += 1
            jitterBuffer.clear()
            opusDecoder.release()
            lastSeq = null
            stateStore.update { it.copy(recentLog = "udp_discontinuity_reset") }
        }

        if (packet.hasConfigChanged) {
            cfgChangedCount += 1
            jitterBuffer.clear()
            lastSeq = null
            resetPlayoutScheduleAfterModeSwitch()
            opusDecoder.release()
            if (options.resetBufferOnSwitch) {
                withPlaybackSinkLock {
                    playbackSink.stop()
                    playbackSink.release()
                    playbackSink = createPlaybackSink()
                    audioInit = false
                    audioStarted = false
                }
            }
            stateStore.update { it.copy(recentLog = "udp_config_changed_resync") }
        }

        val wireBytes = packet.payload.size + packet.headerSize
        val decodeResult = decodePacketForPlayback(packet)
        val playbackPacket = decodeResult.packet ?: run {
            decodeFailCount += 1
            decodeErrorTotal += 1
            maybeLogDecodeAndRxSummary(SystemClock.elapsedRealtime())
            return
        }
        decodeWindowMsSamples.add(decodeResult.decodeMs)
        decodeProducedWindowFrames += 1
        currentPacketCodecLabel = packet.codecLabel
        updateLowLatencyJitterGuard(SystemClock.elapsedRealtime())

        if (lastSeq != null) {
            val expected = ((lastSeq!! + 1L) and 0xFFFFFFFFL).toInt()
            if (playbackPacket.sequence != expected) {
                udpLoss += ((playbackPacket.sequence.toLong() - expected.toLong()) and 0xFFFFFFFFL).toInt()
            }
        }
        lastSeq = playbackPacket.sequence.toLong() and 0xFFFFFFFFL
        jitterBuffer.push(playbackPacket)

        val stats = jitterBuffer.stats
        val jitterBufferedMs = jitterBuffer.bufferedMs()
        val metricsSnapshot = stateStore.current().metrics
        val audioTrackQueuedMs = metricsSnapshot.audioTrackQueuedMs
        val totalBufferedMs = jitterBufferedMs + audioTrackQueuedMs
        stateStore.update {
            val current = it.metrics
            it.copy(
                metrics = current.copy(
                    sampleRate = playbackPacket.sampleRate,
                    channels = playbackPacket.channels,
                    totalBufferedMs = totalBufferedMs,
                    jitterBufferedMs = jitterBufferedMs,
                    audioTrackQueuedMs = audioTrackQueuedMs,
                    jitterUnderrun = stats.underrunCount,
                    jitterDropped = stats.droppedFrames,
                    jitterLate = stats.lateFrames,
                    udpPackets = current.udpPackets + 1,
                    udpBytes = current.udpBytes + wireBytes,
                    lossEstimate = udpLoss,
                    lastSeq = lastSeq,
                    silenceFillCount = silenceFillCount,
                    cfgChangedCount = cfgChangedCount,
                    discontinuityCount = discontinuityCount,
                    jitterP95Ms = jitterP95Ms,
                    jitterHistoryUs = computeJitterHistory(),
                    jitterP50Us = computeJitterP50(),
                    floorHoldCount = stats.floorHoldCount,
                    reconnectCount = reconnectTotalCount,
                    decodeErrors = decodeErrorTotal,
                    sinkWriteGapMsP95 = metricsSnapshot.sinkWriteGapMsP95,
                ),
                protocolVersion = playbackPacket.protocolVersion,
                protocolPath = if (playbackPacket.protocolVersion == 2) "v2_header" else "legacy_las1",
                experimentalPath = playbackPacket.protocolVersion == 2,
                effectiveCodec = packet.codecLabel,
                playbackBackend = playbackBackendLabel(),
            )
        }
        maybeLogDecodeAndRxSummary(SystemClock.elapsedRealtime())
        maybeLogMetrics()
    }

    private fun computeJitterHistory(): List<Int> {
        val count = jitterHistoryIdx.coerceAtMost(120)
        if (count == 0) return emptyList()
        val out = ArrayList<Int>(count)
        val start = if (jitterHistoryIdx > 120) jitterHistoryIdx % 120 else 0
        for (i in 0 until count) {
            out.add(jitterHistoryUs[(start + i) % 120])
        }
        return out
    }

    private fun computeJitterP50(): Int {
        val count = jitterHistoryIdx.coerceAtMost(120)
        if (count == 0) return 0
        val sorted = IntArray(count) { i ->
            val idx = if (jitterHistoryIdx > 120) (jitterHistoryIdx - count + i) % 120 else i
            jitterHistoryUs[idx]
        }
        sorted.sort()
        return sorted[count / 2]
    }

    fun computeJitterP95Us(): Int {
        val count = jitterHistoryIdx.coerceAtMost(120)
        if (count < 2) return if (count == 1) jitterHistoryUs[0] else 0
        val sorted = IntArray(count) { i ->
            val idx = if (jitterHistoryIdx > 120) (jitterHistoryIdx - count + i) % 120 else i
            jitterHistoryUs[idx]
        }
        sorted.sort()
        val p95Idx = (count * 0.95).toInt().coerceAtMost(count - 1)
        return sorted[p95Idx]
    }

    fun getJitterHistorySnapshot(): List<Int> = computeJitterHistory()
    fun getJitterP50Us(): Int = computeJitterP50()

    /**
     * Phase 3: build a watermark sample for the server's adaptive sync
     * engine. Counters are emitted as deltas since the previous report so
     * the server can compute rates. Caller is responsible for invoking this
     * on the same thread as playback state mutates (the ping loop runs on a
     * scheduled executor — a transient stale read is acceptable for control
     * feedback).
     */
    fun buildWatermarkSample(): StreamSessionManager.WatermarkSample {
        val jitterBufMs = jitterBuffer.bufferedMs()
        val ringBufMs = stateStore.current().metrics.audioTrackQueuedMs
        val sinkSilence = sinkSilenceFillTotal
        val underrun = oboeUnderrunCount
        val silenceDelta = (sinkSilence - lastReportedSinkSilenceFillCount).coerceAtLeast(0)
        val underrunDelta = (underrun - lastReportedOboeUnderrunCount).coerceAtLeast(0)
        lastReportedSinkSilenceFillCount = sinkSilence
        lastReportedOboeUnderrunCount = underrun
        return StreamSessionManager.WatermarkSample(
            jitterBufMs = jitterBufMs.coerceAtLeast(0),
            ringBufMs = ringBufMs.coerceAtLeast(0),
            silenceFillDelta = silenceDelta,
            underrunDelta = underrunDelta,
            jitterP95Us = computeJitterP95Us().coerceAtLeast(0),
        )
    }

    private fun decodePacketForPlayback(packet: LasPacket): DecodeResult {
        return when (packet.codec) {
            LasPacket.CODEC_PCM16 -> DecodeResult(packet, 0f)
            LasPacket.CODEC_OPUS_EXPERIMENTAL -> {
                try {
                    val t0 = System.nanoTime()
                    val pcm = opusDecoder.decode(packet)
                    val decodeMs = (System.nanoTime() - t0) / 1_000_000f
                    if (pcm == null) {
                        DecodeResult(null, decodeMs)
                    } else {
                        DecodeResult(packet.copy(payload = pcm), decodeMs)
                    }
                } catch (t: Throwable) {
                    Log.e(logTag, "opus decode failed: ${t.message}")
                    publishError("opus_decode_failed", t.message ?: "opus decode failed")
                    DecodeResult(null, 0f)
                }
            }
            else -> {
                publishError("unsupported_codec", "unsupported codec=${packet.codec}")
                DecodeResult(null, 0f)
            }
        }
    }

    private fun maybeLogDecodeAndRxSummary(nowMs: Long) {
        if (lastDecodeSummaryAtMs == 0L) {
            lastDecodeSummaryAtMs = nowMs
            return
        }
        val elapsed = nowMs - lastDecodeSummaryAtMs
        if (elapsed < DECODE_SUMMARY_INTERVAL_MS) {
            return
        }

        val intervalSec = elapsed.toDouble() / 1000.0
        val avgDecodeMs = averageOrZero(decodeWindowMsSamples)
        val p99DecodeMs = percentileOrZero(decodeWindowMsSamples, 0.99)
        val rxPerSec = if (intervalSec <= 0.0) 0.0 else rxWindowFrames.toDouble() / intervalSec
        val producedPerSec = if (intervalSec <= 0.0) 0.0 else decodeProducedWindowFrames.toDouble() / intervalSec
        val seqLast = rxLastSeqWindow?.toString() ?: "none"

        Log.i(
            decodeLogTag,
            String.format(
                Locale.US,
                "rx_summary interval_5s rx_frames=%d rx_per_sec=%.1f seq_gap_count=%d seq_last=%s",
                rxWindowFrames,
                rxPerSec,
                rxSeqGapCount,
                seqLast,
            ),
        )
        Log.i(
            decodeLogTag,
            String.format(
                Locale.US,
                "decode_summary interval_5s produced=%d produced_per_sec=%.1f decode_avg_ms=%.3f decode_p99_ms=%.3f decode_fail_count=%d",
                decodeProducedWindowFrames,
                producedPerSec,
                avgDecodeMs,
                p99DecodeMs,
                decodeFailCount,
            ),
        )

        lastDecodeSummaryAtMs = nowMs
        decodeWindowMsSamples.clear()
        decodeProducedWindowFrames = 0
        decodeFailCount = 0
        rxWindowFrames = 0
        rxSeqGapCount = 0
        rxLastSeqWindow = null
    }

    private fun averageOrZero(samples: List<Float>): Double {
        if (samples.isEmpty()) {
            return 0.0
        }
        return samples.sumOf { it.toDouble() } / samples.size.toDouble()
    }

    private fun percentileOrZero(samples: List<Float>, percentile: Double): Double {
        if (samples.isEmpty()) {
            return 0.0
        }
        val sorted = samples.sorted()
        val p = percentile.coerceIn(0.0, 1.0)
        val idx = kotlin.math.ceil(sorted.size * p).toInt().coerceIn(1, sorted.size) - 1
        return sorted[idx].toDouble()
    }

    private fun playoutTick() {
        val now = SystemClock.elapsedRealtime()
        if (playbackStateStableSinceMs == 0L) {
            playbackStateStableSinceMs = now
        }
        if (nextPlayoutAtMs > 0L && now < nextPlayoutAtMs) {
            maybeLogMetrics()
            return
        }
        if (modeSwitchInFlight) {
            if (!tryCompleteHighQualityPrefill(now)) {
                maybeLogMetrics()
                return
            }
        }

        val usbLowLatencyHardFloor = currentTransportHint == TransportHint.Usb &&
            PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode) == "low_latency"
        val frame = jitterBuffer.pop(usbLowLatencyHardFloor)
        val stats = jitterBuffer.stats
        if (frame == null) {
            consecutivePlayoutHits = 0
            consecutivePlayoutMisses += 1
            val audioStats = withPlaybackSinkLock { playbackSink.stats() }
            updateSilenceFillAccounting(audioStats.silenceFillTotal.toInt(), now)
            oboeUnderrunCount = audioStats.underrunTotal
            val audioQueuedMs = audioStats.reportedLatencyMs ?: framesToMs(audioStats.nativeQueuedAudioFrames)
            val totalBufferedMs = jitterBuffer.bufferedMs() + audioQueuedMs
            val hasQueuedAudio = audioStarted && audioStats.nativeQueuedAudioFrames > 0
            if (hasQueuedAudio) {
                consecutiveEmptyQueueMisses = 0
                bufferingCandidateSinceMs = 0
            } else {
                consecutiveEmptyQueueMisses += 1
                if (bufferingCandidateSinceMs == 0L) {
                    bufferingCandidateSinceMs = now
                }
            }
            val frameDurationMs = options.frameDurationMs.coerceAtLeast(10)
            val targetFramesForStateSwitch = ((options.startBufferMs + frameDurationMs - 1) / frameDurationMs)
                .coerceIn(options.batchFrames.coerceAtLeast(1), 24)
            val bufferingThreshold = if (options.preferLowLatencyPath) {
                options.batchFrames.coerceIn(1, 4)
            } else {
                targetFramesForStateSwitch
            }
            val starvationMs = if (bufferingCandidateSinceMs > 0L) now - bufferingCandidateSinceMs else 0L
            val bufferingDelayMs = options.bufferingEnterDelayMs.toLong().coerceIn(80L, 1200L)
            val enterBufferFloorMs = (options.startBufferMs / 2).coerceAtLeast(options.frameDurationMs)
            val shouldEnterBuffering = consecutiveEmptyQueueMisses >= bufferingThreshold &&
                starvationMs >= bufferingDelayMs &&
                totalBufferedMs <= enterBufferFloorMs
            stateStore.update {
                val nextPlaybackState = stabilizedPlaybackState(
                    current = it.playbackState,
                    desired = if (shouldEnterBuffering) "buffering" else it.playbackState,
                    now = now,
                )
                it.copy(
                    playbackState = nextPlaybackState,
                    metrics = it.metrics.copy(
                        totalBufferedMs = totalBufferedMs,
                        jitterBufferedMs = jitterBuffer.bufferedMs(),
                        audioTrackQueuedMs = audioQueuedMs,
                        jitterUnderrun = stats.underrunCount,
                        jitterDropped = stats.droppedFrames,
                        jitterLate = stats.lateFrames,
                        silenceFillCount = silenceFillCount,
                        startupSilenceFillCount = startupSilenceFillCount,
                        cfgChangedCount = cfgChangedCount,
                        discontinuityCount = discontinuityCount,
                        jitterP95Ms = jitterP95Ms,
                        floorHoldCount = stats.floorHoldCount,
                        reconnectCount = reconnectTotalCount,
                        decodeErrors = decodeErrorTotal,
                        sinkWriteGapMsP95 = audioStats.writeGapP95Ms,
                    ),
                )
            }
            maybeLogMetrics()
            return
        }
        consecutivePlayoutMisses = 0
        consecutiveEmptyQueueMisses = 0
        bufferingCandidateSinceMs = 0
        consecutivePlayoutHits += 1
        lastAudioFormat = ActiveAudioFormat(
            sampleRate = frame.sampleRate,
            channels = frame.channels,
            frameSamplesPerChannel = frame.frameDurationMs * frame.sampleRate / 1000,
        )

        if (!audioInit) {
            Log.i(logTag, "init AudioTrack sr=${frame.sampleRate} ch=${frame.channels}")
            withPlaybackSinkLock {
                initPlaybackSink(lastAudioFormat!!)
                audioInit = true
            }
        }
        val preWriteAudioQueuedMs = stateStore.current().metrics.audioTrackQueuedMs
        val writeBatch = collectWritePayload(
            first = frame,
            audioQueuedMs = preWriteAudioQueuedMs,
            jitterBufferedMs = jitterBuffer.bufferedMs(),
        )
        loudnessNormalizer.configure(frame.sampleRate, frame.channels)
        val processedPayload = loudnessNormalizer.process(writeBatch.payload, now)
        val audioStats = withPlaybackSinkLock {
            if (!audioStarted) {
                playbackSink.start()
                audioStarted = true
            }
            playbackSink.writePcm16(processedPayload, writeBatch.pcmFrames)
            playbackSink.stats()
        }
        val writtenPacketCount = writeBatch.packetCount.coerceAtLeast(1)
        val frameDurationMs = frame.frameDurationMs.takeIf { it > 0 } ?: 10
        oboeUnderrunCount = audioStats.underrunTotal
        val audioQueuedMs = audioStats.reportedLatencyMs ?: framesToMs(audioStats.nativeQueuedAudioFrames)
        val jitterBufferedMs = jitterBuffer.bufferedMs()
        var totalBufferedMs = jitterBufferedMs + audioQueuedMs
        val latencyGuardCooldownMs = 220L
        val latencyGuardObserveWindowMs = if (options.preferLowLatencyPath) 0L else LATENCY_GUARD_OBSERVE_WINDOW_MS
        val overflowActive = totalBufferedMs > options.maxTotalLatencyMs
        if (overflowActive) {
            if (latencyGuardOverflowSinceMs == 0L) {
                latencyGuardOverflowSinceMs = now
            }
        } else {
            latencyGuardOverflowSinceMs = 0L
        }
        val trackQueueReadyForTrim = audioQueuedMs >= balancedAudioQueueLowWatermarkMs(frameDurationMs)
        val guardMayTrim = options.preferLowLatencyPath ||
            ((latencyGuardOverflowSinceMs > 0L) &&
                (now - latencyGuardOverflowSinceMs >= latencyGuardObserveWindowMs) &&
                jitterBufferedMs >= options.startBufferMs &&
                trackQueueReadyForTrim)
        if (overflowActive && guardMayTrim && now - lastLatencyGuardAtMs >= latencyGuardCooldownMs) {
            val dropMs = (totalBufferedMs - options.targetTotalLatencyMs)
                .coerceAtLeast(frameDurationMs)
                .coerceAtMost(if (options.preferLowLatencyPath) frameDurationMs * 3 else frameDurationMs * 2)
            val dropped = jitterBuffer.dropOldestMs(dropMs)
            if (dropped > 0) {
                lastLatencyGuardAtMs = now
                latencyGuardOverflowSinceMs = 0L
                latencyGuardDropCount += 1
                totalBufferedMs = jitterBuffer.bufferedMs() + audioQueuedMs
                stateStore.update { it.copy(recentLog = "latency_guard_drop:$dropped") }
            }
        } else if (overflowActive && !options.preferLowLatencyPath && latencyGuardOverflowSinceMs > 0L) {
            val overflowMs = now - latencyGuardOverflowSinceMs
            if (overflowMs in 1 until LATENCY_GUARD_OBSERVE_WINDOW_MS &&
                now - lastLatencyGuardAtMs >= latencyGuardCooldownMs
            ) {
                stateStore.update { it.copy(recentLog = "latency_guard_hold:${totalBufferedMs}") }
            }
        }
        val catchupMs = (totalBufferedMs - options.targetTotalLatencyMs).coerceAtLeast(0)
        val effectiveIntervalMs = (writtenPacketCount * frameDurationMs +
            pacingOffsetMs(
                totalBufferedMs = totalBufferedMs,
                catchupMs = catchupMs,
                frameDurationMs = frameDurationMs,
                audioQueuedMs = audioQueuedMs,
            )).coerceIn(
                frameDurationMs,
                writtenPacketCount * frameDurationMs + BALANCED_PACING_SLOWDOWN_MAX_MS,
            )
        nextPlayoutAtMs = now + effectiveIntervalMs.toLong()
        val resumeThreshold = if (options.preferLowLatencyPath) {
            1
        } else {
            options.batchFrames.coerceIn(2, 4)
        }
        val resumeBufferFloorMs = (options.startBufferMs / 2).coerceAtLeast(frameDurationMs)
        stateStore.update {
            val shouldResumePlaying = it.playbackState != "buffering" ||
                (consecutivePlayoutHits >= resumeThreshold && totalBufferedMs >= resumeBufferFloorMs)
            val nextPlaybackState = stabilizedPlaybackState(
                current = it.playbackState,
                desired = if (shouldResumePlaying) "playing" else "buffering",
                now = now,
            )
            updateSilenceFillAccounting(
                sinkSilenceFillTotal = audioStats.silenceFillTotal.toInt(),
                nowMs = now,
                playbackState = nextPlaybackState,
            )
            it.copy(
                playbackState = nextPlaybackState,
                metrics = it.metrics.copy(
                    sampleRate = frame.sampleRate,
                    channels = frame.channels,
                    totalBufferedMs = totalBufferedMs,
                    jitterBufferedMs = jitterBuffer.bufferedMs(),
                    audioTrackQueuedMs = audioQueuedMs,
                    audioTrackLatencyMs = audioStats.reportedLatencyMs,
                    jitterUnderrun = stats.underrunCount,
                    jitterDropped = stats.droppedFrames,
                    jitterLate = stats.lateFrames,
                    nativeQueuedFrames = audioStats.nativeQueuedFrames,
                    audioTrackWriteFrames = audioStats.audioTrackWriteFrames,
                    audioTrackShortWriteCount = audioStats.audioTrackShortWriteCount,
                    silenceFillCount = silenceFillCount,
                    startupSilenceFillCount = startupSilenceFillCount,
                    cfgChangedCount = cfgChangedCount,
                    discontinuityCount = discontinuityCount,
                    jitterP95Ms = jitterP95Ms,
                    floorHoldCount = stats.floorHoldCount,
                    loudnessGainDb = loudnessNormalizer.gainDb(),
                ),
                playbackBackend = playbackBackendLabel(),
            )
        }
        maybeLogMetrics()
    }

    private fun scheduleReconnect(reason: String, immediate: Boolean = false) {
        val target = currentTarget ?: return
        if (immediate) {
            reconnectAttemptCount = 0
        }
        if (!immediate && reconnectAttemptCount >= MAX_AUTO_RECONNECT_ATTEMPTS) {
            Log.w(logTag, "auto reconnect exhausted reason=$reason attempts=$reconnectAttemptCount")
            reconnectFuture?.cancel(false)
            reconnectFuture = null
            stopStreamAndAudio()
            stateStore.update {
                it.copy(
                    serviceState = "error",
                    connectionState = "error",
                    playbackState = "stopped",
                    reconnectAttempts = reconnectAttemptCount,
                    reconnectDelayMs = 0,
                    recentLog = "reconnect_exhausted:$reason",
                    error = mapOf(
                        "code" to "reconnect_exhausted",
                        "message" to "reconnect failed after $MAX_AUTO_RECONNECT_ATTEMPTS attempts: $reason",
                    ),
                )
            }
            return
        }

        val attemptNumber = if (immediate) 0 else reconnectAttemptCount + 1
        if (!immediate) {
            reconnectAttemptCount = attemptNumber
            reconnectTotalCount += 1
        }
        val delayMs = if (immediate) 0L else reconnectDelayMs(attemptNumber)
        Log.w(
            logTag,
            "scheduleReconnect reason=$reason immediate=$immediate attempt=$attemptNumber/$MAX_AUTO_RECONNECT_ATTEMPTS delayMs=$delayMs",
        )
        reconnectFuture?.cancel(false)
        val generation = ++streamGeneration
        stateStore.update {
            it.copy(
                serviceState = "running",
                connectionState = "reconnecting",
                playbackState = "buffering",
                reconnectAttempts = attemptNumber,
                reconnectDelayMs = delayMs.toInt(),
                recentLog = if (immediate) {
                    "reconnect:$reason"
                } else {
                    "reconnect_wait:$attemptNumber/$MAX_AUTO_RECONNECT_ATTEMPTS:${delayMs}ms:$reason"
                },
            )
        }
        reconnectFuture = controlExecutor.schedule({
            if (generation != streamGeneration) {
                return@schedule
            }
            stateStore.update {
                it.copy(
                    serviceState = "running",
                    connectionState = "reconnecting",
                    playbackState = "buffering",
                    reconnectAttempts = attemptNumber,
                    reconnectDelayMs = 0,
                    recentLog = if (immediate) "reconnect:$reason" else "reconnect:$attemptNumber/$MAX_AUTO_RECONNECT_ATTEMPTS:$reason",
                )
            }
            streamManager?.stop()
            streamManager = null
            withPlaybackSinkLock {
                playbackSink.stop()
                playbackSink.release()
                playbackSink = createPlaybackSink()
                audioInit = false
                audioStarted = false
            }
            opusDecoder.release()
            jitterBuffer.clear()
            streamManager = StreamSessionManager(
                target,
                options.pingIntervalMs,
                stateStore.current().currentAudioMode,
                object : StreamSessionManager.Callback {
                override fun onLog(message: String) {
                    if (generation != streamGeneration) return
                    stateStore.update { it.copy(recentLog = message) }
                }

                override fun onWsConnected() {
                    if (generation != streamGeneration) return
                    reconnectFuture?.cancel(false)
                    reconnectFuture = null
                    reconnectAttemptCount = 0
                    stateStore.update {
                        it.copy(
                            connectionState = "connected",
                            playbackState = "buffering",
                            reconnectAttempts = 0,
                            reconnectDelayMs = 0,
                            recentLog = "ws_reconnected",
                            error = null,
                        )
                    }
                }

                override fun onWsDisconnected(reason: String) {
                    if (generation != streamGeneration) return
                    scheduleReconnect(reason)
                }

                override fun onUdpPacket(packet: LasPacket) {
                    if (generation != streamGeneration) return
                    handleUdpPacket(packet)
                }

                override fun onControlHelloAck(
                    protocolVersion: Int,
                    currentAudioMode: String,
                    capabilities: Map<String, Boolean>,
                    transportType: String,
                ) {
                    if (generation != streamGeneration) return
                    applyAudioModeProfile(currentAudioMode, "hello_ack", transportType)
                    stateStore.update {
                        it.copy(
                            protocolVersion = protocolVersion,
                            currentAudioMode = PlaybackModeProfiles.normalize(currentAudioMode),
                            modeProfile = PlaybackModeProfiles.forMode(
                                currentAudioMode,
                                currentTransportHint,
                            ),
                            transportMode = if (currentTransportHint == TransportHint.Usb) "usb" else "wifi",
                            negotiatedCapabilities = capabilities,
                            recentLog = "hello_ack_v2",
                        )
                    }
                }

                override fun onServerInfo(platform: String?, appVersion: String?, currentAudioMode: String) {
                    if (generation != streamGeneration) return
                    applyAudioModeProfile(currentAudioMode, "server_info")
                    stateStore.update {
                        it.copy(
                            serverPlatform = platform,
                            serverAppVersion = appVersion,
                            currentAudioMode = PlaybackModeProfiles.normalize(currentAudioMode),
                            modeProfile = PlaybackModeProfiles.forMode(
                                currentAudioMode,
                                currentTransportHint,
                            ),
                        )
                    }
                }

                override fun onAudioModeChanged(mode: String, applied: Boolean, reason: String) {
                    if (generation != streamGeneration) return
                    if (applied) {
                        applyAudioModeProfile(mode, reason)
                    }
                    stateStore.update {
                        it.copy(
                            currentAudioMode = PlaybackModeProfiles.normalize(mode),
                            modeProfile = PlaybackModeProfiles.forMode(mode, currentTransportHint),
                            recentLog = if (applied) "audio_mode_changed:$mode" else "audio_mode_rejected:$reason",
                        )
                    }
                }

                override fun onClientCountUpdated(count: Int) {
                    if (generation != streamGeneration) return
                    stateStore.update {
                        it.copy(connectedClientCount = count.coerceAtLeast(0))
                    }
                }

                override fun onTcpRoundTripMs(roundTripMs: Int, medianMs: Int) {
                    if (generation != streamGeneration) return
                    stateStore.update {
                        it.copy(
                            metrics = it.metrics.copy(
                                tcpRoundTripMs = roundTripMs,
                                tcpRoundTripMedianMs = medianMs,
                            ),
                        )
                    }
                }

                override fun onError(code: String, message: String) {
                    if (generation != streamGeneration) return
                    publishError(code, message)
                    scheduleReconnect(code)
                }

                override fun provideWatermark(): StreamSessionManager.WatermarkSample? {
                    if (generation != streamGeneration) return null
                    return try {
                        buildWatermarkSample()
                    } catch (t: Throwable) {
                        Log.w(logTag, "buildWatermarkSample failed: ${t.message}")
                        null
                    }
                }
            })
            streamManager?.start()
            ensurePlayoutLoop()
        }, delayMs, TimeUnit.MILLISECONDS)
    }

    private fun stopStreamAndAudio() {
        streamGeneration += 1
        streamManager?.stop()
        streamManager = null
        playoutFuture?.cancel(true)
        playoutFuture = null
        opusDecoder.release()
        withPlaybackSinkLock {
            playbackSink.stop()
            playbackSink.release()
            playbackSink = createPlaybackSink()
            audioInit = false
            audioStarted = false
        }
        currentPacketCodecLabel = "pcm16"
        lastAudioFormat = null
        nextPlayoutAtMs = 0
        consecutivePlayoutMisses = 0
        consecutiveEmptyQueueMisses = 0
        consecutivePlayoutHits = 0
        bufferingCandidateSinceMs = 0
        playbackStateStableSinceMs = 0
        lastLatencyGuardAtMs = 0
        latencyGuardOverflowSinceMs = 0
        latencyGuardDropCount = 0
        lastPacketArrivalAtMs = 0
        recentArrivalIntervalsMs.clear()
        jitterP95Ms = null
        adaptiveStartBufferMs = null
        adaptiveStableSinceMs = 0
        lastRxSeqRaw = null
        rxSeqGapCount = 0
        rxLastSeqWindow = null
        decodeProducedWindowFrames = 0
        decodeFailCount = 0
        decodeErrorTotal = 0
        rxWindowFrames = 0
        decodeWindowMsSamples.clear()
        jitterBuffer.clear()
        oboeUnderrunCount = 0
        sinkSilenceFillTotal = 0
        startupSilenceFillCount = 0
        startupSilenceFillBaseline = 0
        startupSilenceTrackingActive = false
        startupSilencePhaseUntilMs = 0L
        smoothedRxFramesPerSec = null
        lastDiagnosedSilenceFillCount = 0
        lastDiagnosedSinkSilenceFillCount = 0
        lastDiagnosedOboeUnderrunCount = 0
        lastReportedSinkSilenceFillCount = 0
        lastReportedOboeUnderrunCount = 0
        balancedQueueBelowFillTarget = false
        highQualityPrefillActive = false
        highQualityPrefillDeadlineMs = 0L
        highQualityPrefillTargetMs = 0
        modeSwitchInFlight = false
    }

    private fun publishError(code: String, message: String) {
        Log.e(logTag, "publishError code=$code message=$message")
        stateStore.update {
            it.copy(
                serviceState = "error",
                connectionState = "error",
                playbackState = "stopped",
                recentLog = "$code:$message",
                error = mapOf("code" to code, "message" to message),
            )
        }
    }

    private fun registerNoisyReceiver() {
        if (noisyRegistered) {
            return
        }
        context.registerReceiver(noisyReceiver, IntentFilter(AudioManager.ACTION_AUDIO_BECOMING_NOISY))
        noisyRegistered = true
    }

    private fun unregisterNoisyReceiver() {
        if (!noisyRegistered) {
            return
        }
        try {
            context.unregisterReceiver(noisyReceiver)
        } catch (_: Throwable) {
        } finally {
            noisyRegistered = false
        }
    }

    private fun requestAudioFocus(): Boolean {
        if (audioFocusAcquired) {
            return true
        }
        val focusListener = AudioManager.OnAudioFocusChangeListener { change ->
            when (change) {
                AudioManager.AUDIOFOCUS_LOSS,
                AudioManager.AUDIOFOCUS_LOSS_TRANSIENT -> stopPlayback("audio_focus_lost")
            }
        }
        legacyFocusListener = focusListener
        val granted = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val request = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN)
                .setAudioAttributes(
                    AudioAttributes.Builder()
                        .setUsage(AudioAttributes.USAGE_MEDIA)
                        .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                        .build(),
                )
                .setOnAudioFocusChangeListener(focusListener)
                .build()
            focusRequest = request
            audioManager.requestAudioFocus(request) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        } else {
            @Suppress("DEPRECATION")
            audioManager.requestAudioFocus(
                focusListener,
                AudioManager.STREAM_MUSIC,
                AudioManager.AUDIOFOCUS_GAIN,
            ) == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        }
        audioFocusAcquired = granted
        Log.i(logTag, "audio focus acquired=$granted")
        return granted
    }

    private fun releaseAudioFocus() {
        if (!audioFocusAcquired) {
            return
        }
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            focusRequest?.let { audioManager.abandonAudioFocusRequest(it) }
        } else {
            @Suppress("DEPRECATION")
            audioManager.abandonAudioFocus(legacyFocusListener)
        }
        focusRequest = null
        legacyFocusListener = null
        audioFocusAcquired = false
        Log.i(logTag, "audio focus released")
    }

    private fun applyAudioModeProfile(mode: String, reason: String, transportType: String? = null) {
        synchronized(modeSwitchLock) {
            val previousBackendConfig = currentAudioBackendConfig()
            val previousMode = PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode)
            transportType?.let {
                currentTransportHint = transportHintFromWire(it)
            }
            val normalized = PlaybackModeProfiles.normalize(mode)
            val profile = PlaybackModeProfiles.forMode(normalized, currentTransportHint)
            options = profile.toOptions(options.pingIntervalMs)
            loudnessNormalizer.setMode(normalized)
            val nextBackendConfig = currentAudioBackendConfig()
            if (normalized != "low_latency") {
                adaptiveStartBufferMs = null
                adaptiveStableSinceMs = 0
                recentArrivalIntervalsMs.clear()
                jitterP95Ms = null
            }
            balancedQueueBelowFillTarget = false
            jitterBuffer.reconfigure(options.startBufferMs, options.maxBufferMs, options.dropThresholdMs)
            withPlaybackSinkLock {
                playbackSink.setQueueSoftCapFrames(msToFrames(options.audioQueueSoftCapMs))
            }
            val streamActive = streamManager != null
            val modeChanged = previousMode != normalized
            val backendChanged = previousBackendConfig != nextBackendConfig
            val shouldResetActiveModeSwitch = streamActive && modeChanged
            if (shouldResetActiveModeSwitch) {
                reinitAudioBackend(
                    mode = normalized,
                    reason = "$reason:mode_switch_reset",
                    resetJitterState = true,
                )
            } else if (streamActive && audioInit && backendChanged) {
                reinitAudioBackend(normalized, reason, resetJitterState = false)
            } else if (modeChanged && profile.resetBufferOnSwitch && !streamActive) {
                jitterBuffer = newJitterBuffer(options)
                withPlaybackSinkLock {
                    playbackSink.stop()
                    playbackSink.release()
                    playbackSink = createPlaybackSink()
                    audioInit = false
                    audioStarted = false
                }
                resetJitterAfterModeSwitch("$reason:idle_mode_switch")
            } else if (streamActive && backendChanged) {
                Log.i(
                    logTag,
                    "audio_backend_reinit_pending mode=$normalized transport=${currentTransportHint.name.lowercase()} reason=$reason audioInit=$audioInit",
                )
            } else if (modeChanged && profile.resetBufferOnSwitch) {
                Log.i(
                    logTag,
                    "defer playback reset to udp discontinuity mode=$normalized reason=$reason",
                )
            }
            Log.i(
                logTag,
                "applyAudioModeProfile mode=$normalized transport=${currentTransportHint.name.lowercase()} reason=$reason startBufferMs=${options.startBufferMs} maxBufferMs=${options.maxBufferMs} batchFrames=${options.batchFrames} dropThresholdMs=${options.dropThresholdMs}"
            )
        }
    }

    private fun transportHintFromWire(value: String): TransportHint {
        return if (value.equals("usb", ignoreCase = true)) TransportHint.Usb else TransportHint.Wifi
    }

    private fun updateLowLatencyJitterGuard(nowMs: Long) {
        val interval = if (lastPacketArrivalAtMs > 0L) {
            (nowMs - lastPacketArrivalAtMs).coerceAtLeast(0L).toInt()
        } else {
            null
        }
        lastPacketArrivalAtMs = nowMs
        if (interval != null) {
            if (recentArrivalIntervalsMs.size >= JITTER_P95_WINDOW_FRAMES) {
                recentArrivalIntervalsMs.removeFirst()
            }
            recentArrivalIntervalsMs.addLast(interval)
        }

        if (PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode) != "low_latency" &&
            PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode) != "ultra_low_latency") {
            adaptiveStableSinceMs = 0
            adaptiveStartBufferMs = null
            jitterP95Ms = null
            return
        }

        // Ultra-low-latency auto-degradation: if jitter p95 exceeds 20ms
        // for more than 5 seconds after mode switch, switch to low_latency.
        if (PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode) == "ultra_low_latency") {
            // Grace period: don't degrade within 5 seconds of mode switch
            // (jitter data from previous mode is still in the window).
            if (recentArrivalIntervalsMs.size < JITTER_P95_WINDOW_FRAMES) {
                return
            }
            val p95 = percentile95(recentArrivalIntervalsMs)
            jitterP95Ms = p95
            if (p95 > 20) {
                // Require sustained high jitter (use adaptiveStableSinceMs as "bad since" tracker)
                if (adaptiveStableSinceMs == 0L) {
                    adaptiveStableSinceMs = nowMs
                } else if (nowMs - adaptiveStableSinceMs >= 3000L) {
                    Log.w(logTag, "ultra_low_latency auto-degrade: jitter p95=${p95}ms > 20ms for 3s, switching to low_latency")
                    adaptiveStableSinceMs = 0
                    setAudioMode("low_latency", reason = "auto_degraded_jitter")
                }
            } else {
                adaptiveStableSinceMs = 0
            }
            // For ultra_low_latency, don't apply the adaptive buffer boost — just degrade.
            return
        }
        if (recentArrivalIntervalsMs.isEmpty()) {
            return
        }
        val p95 = percentile95(recentArrivalIntervalsMs)
        jitterP95Ms = p95
        val triggerThreshold = options.dropThresholdMs * 0.8
        val capStartBufferMs = WIFI_BALANCED_MAX_BUFFER_CAP_MS
        if (p95 > triggerThreshold) {
            adaptiveStableSinceMs = 0
            val baseStartBufferMs = PlaybackModeProfiles
                .forMode("low_latency", currentTransportHint)
                .startBufferMs
            val boostedStartBufferMs = (baseStartBufferMs + LOW_LATENCY_ADAPTIVE_BOOST_MS)
                .coerceAtMost(capStartBufferMs)
            if (adaptiveStartBufferMs != boostedStartBufferMs) {
                adaptiveStartBufferMs = boostedStartBufferMs
                applyAdaptiveStartBuffer(boostedStartBufferMs, p95)
            }
            return
        }

        if (adaptiveStartBufferMs == null) {
            adaptiveStableSinceMs = 0
            return
        }
        if (adaptiveStableSinceMs == 0L) {
            adaptiveStableSinceMs = nowMs
            return
        }
        if (nowMs - adaptiveStableSinceMs >= LOW_LATENCY_RECOVER_MS) {
            adaptiveStableSinceMs = 0
            adaptiveStartBufferMs = null
            val base = PlaybackModeProfiles.forMode("low_latency", currentTransportHint)
            options = base.toOptions(options.pingIntervalMs)
            jitterBuffer.reconfigure(options.startBufferMs, options.maxBufferMs, options.dropThresholdMs)
            Log.i(logTag, "low_latency_jitter_guard_recover startBufferMs=${options.startBufferMs}")
        }
    }

    private fun applyAdaptiveStartBuffer(startBufferMs: Int, p95Ms: Int) {
        options = options.copy(startBufferMs = startBufferMs)
        jitterBuffer.reconfigure(options.startBufferMs, options.maxBufferMs, options.dropThresholdMs)
        Log.w(
            logTag,
            "low_latency_jitter_guard_boost startBufferMs=$startBufferMs p95_ms=$p95Ms dropThresholdMs=${options.dropThresholdMs}",
        )
    }

    private fun percentile95(samples: Collection<Int>): Int {
        if (samples.isEmpty()) {
            return 0
        }
        val sorted = samples.sorted()
        val idx = kotlin.math.ceil(sorted.size * 0.95).toInt().coerceIn(1, sorted.size) - 1
        return sorted[idx].coerceAtLeast(0)
    }

    private fun newJitterBuffer(opts: PlaybackOptions): PlaybackJitterBuffer {
        return PlaybackJitterBuffer(opts.startBufferMs, opts.maxBufferMs, opts.dropThresholdMs)
    }

    private fun pacingOffsetMs(
        totalBufferedMs: Int,
        catchupMs: Int,
        frameDurationMs: Int,
        audioQueuedMs: Int,
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

    private fun balancedAudioQueueLowWatermarkMs(frameDurationMs: Int): Int {
        return (frameDurationMs * 2 + BALANCED_AUDIO_QUEUE_LOW_WATERMARK_EXTRA_MS)
            .coerceAtLeast(BUFFER_EMPTY_LOW_WATERMARK_MS)
    }

    private fun balancedAudioQueueFillTargetMs(frameDurationMs: Int): Int {
        return balancedAudioQueueLowWatermarkMs(frameDurationMs) + BALANCED_AUDIO_QUEUE_FILL_TARGET_EXTRA_MS
    }

    private fun modeProfileForOptions(
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

    private fun createPlaybackSink(): PlaybackAudioSink {
        return if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) {
            OboeAudioTrackController()
        } else {
            AudioTrackController()
        }
    }

    private fun initPlaybackSink(format: ActiveAudioFormat) {
        val preferredSink = createPlaybackSink()
        val initializedSink = try {
            preferredSink.init(
                sampleRate = format.sampleRate,
                channels = format.channels,
                frameSamplesPerChannel = format.frameSamplesPerChannel,
                transportHint = currentTransportHint,
                encoding = AudioFormat.ENCODING_PCM_16BIT,
            )
            preferredSink
        } catch (t: Throwable) {
            preferredSink.release()
            if (preferredSink is AudioTrackController) {
                throw t
            }
            Log.w(
                logTag,
                "oboe_open_failed_fallback transport=${currentTransportHint.name.lowercase()} reason=${t.message}",
                t,
            )
            AudioTrackController().also {
                it.init(
                    sampleRate = format.sampleRate,
                    channels = format.channels,
                    frameSamplesPerChannel = format.frameSamplesPerChannel,
                    transportHint = currentTransportHint,
                    encoding = AudioFormat.ENCODING_PCM_16BIT,
                )
            }
        }
        playbackSink = initializedSink
        playbackSink.setQueueSoftCapFrames(msToFrames(options.audioQueueSoftCapMs))
        playbackSink.setEqSettings(eqSettings)
        stateStore.update {
            it.copy(playbackBackend = playbackBackendLabel())
        }
    }

    private fun currentAudioBackendConfig(): AudioBackendConfig {
        return AudioBackendConfig(
            transportHint = currentTransportHint,
            preferLowLatencyPath = options.preferLowLatencyPath,
            lowLatencyBufferMultiplier = options.lowLatencyBufferMultiplier,
            lowLatencyFallbackBufferMultiplier = options.lowLatencyFallbackBufferMultiplier,
            preferredBackend = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O_MR1) "oboe" else "audiotrack",
        )
    }

    private fun reinitAudioBackend(
        mode: String,
        reason: String,
        resetJitterState: Boolean = false,
    ) {
        val format = lastAudioFormat
        if (format == null) {
            Log.w(
                logTag,
                "audio_backend_reinit_skipped mode=$mode transport=${currentTransportHint.name.lowercase()} reason=$reason missing_format=true",
            )
            if (resetJitterState) {
                resetJitterAfterModeSwitch(reason)
            }
            return
        }
        Log.i(
            logTag,
            "audio_backend_reinit mode=$mode transport=${currentTransportHint.name.lowercase()} reason=$reason sampleRate=${format.sampleRate} channels=${format.channels} frameSamples=${format.frameSamplesPerChannel}",
        )
        var shouldResumeOnNextWrite = false
        var keepModeSwitchGuard = false
        val normalizedMode = PlaybackModeProfiles.normalize(mode)
        highQualityPrefillActive = false
        highQualityPrefillDeadlineMs = 0L
        highQualityPrefillTargetMs = 0
        modeSwitchInFlight = true
        try {
            withPlaybackSinkLock {
                shouldResumeOnNextWrite = audioStarted || streamManager != null
                playbackSink.stop()
                playbackSink.release()
                initPlaybackSink(format)
                audioInit = true
                audioStarted = false
            }
            if (resetJitterState) {
                resetJitterAfterModeSwitch(reason)
            }
            if (resetJitterState && normalizedMode == "high_quality" && streamManager != null) {
                beginHighQualityPrefillWindow(reason)
                keepModeSwitchGuard = true
            } else if (resetJitterState && normalizedMode == "low_latency") {
                beginModeSwitchStartupSilenceWindow(normalizedMode, reason)
            }
        } finally {
            modeSwitchInFlight = keepModeSwitchGuard
        }
        Log.i(
            logTag,
            "audio_backend_reinit_done mode=$mode transport=${currentTransportHint.name.lowercase()} reason=$reason resume_on_next_write=$shouldResumeOnNextWrite reset_jitter=$resetJitterState hq_prefill=$keepModeSwitchGuard",
        )
    }

    private fun beginHighQualityPrefillWindow(reason: String) {
        val now = SystemClock.elapsedRealtime()
        highQualityPrefillTargetMs = options.startBufferMs.coerceAtLeast(options.frameDurationMs)
        highQualityPrefillDeadlineMs = now + HIGH_QUALITY_PREFILL_MAX_WAIT_MS
        highQualityPrefillActive = true

        // Route callback silence during HQ mode-switch warmup into startup accounting.
        beginModeSwitchStartupSilenceWindow("high_quality", reason)

        Log.i(
            logTag,
            "hq_mode_switch_prefill_start reason=$reason target_ms=$highQualityPrefillTargetMs timeout_ms=$HIGH_QUALITY_PREFILL_MAX_WAIT_MS",
        )
    }

    private fun beginModeSwitchStartupSilenceWindow(mode: String, reason: String) {
        startupSilenceTrackingActive = true
        startupSilenceFillBaseline = sinkSilenceFillTotal
        startupSilenceFillCount = sinkSilenceFillTotal
        startupSilencePhaseUntilMs = 0L
        Log.i(logTag, "mode_switch_startup_silence_window_start mode=$mode reason=$reason")
    }

    private fun tryCompleteHighQualityPrefill(nowMs: Long): Boolean {
        if (!highQualityPrefillActive) {
            return false
        }
        val jitterBufferedMs = jitterBuffer.bufferedMs()
        val reachedTarget = jitterBufferedMs >= highQualityPrefillTargetMs
        val timedOut = nowMs >= highQualityPrefillDeadlineMs
        if (!reachedTarget && !timedOut) {
            return false
        }

        highQualityPrefillActive = false
        highQualityPrefillDeadlineMs = 0L
        highQualityPrefillTargetMs = 0
        modeSwitchInFlight = false

        Log.i(
            logTag,
            "hq_mode_switch_prefill_done reason=${if (reachedTarget) "target_reached" else "timeout"} jitter_buffered_ms=$jitterBufferedMs",
        )
        return true
    }

    private fun resetJitterAfterModeSwitch(reason: String) {
        jitterBuffer.clear()
        opusDecoder.release()
        lastSeq = null
        resetPlayoutScheduleAfterModeSwitch()
        Log.i(logTag, "mode_switch_jitter_reset reason=$reason")
    }

    private fun resetPlayoutScheduleAfterModeSwitch() {
        nextPlayoutAtMs = 0
        consecutivePlayoutMisses = 0
        consecutiveEmptyQueueMisses = 0
        consecutivePlayoutHits = 0
        bufferingCandidateSinceMs = 0
        balancedQueueBelowFillTarget = false
    }

    private inline fun <T> withPlaybackSinkLock(block: () -> T): T {
        return synchronized(playbackSinkLock) {
            block()
        }
    }

    private fun msToFrames(ms: Int): Int {
        val safeFrameMs = options.frameDurationMs.coerceAtLeast(10)
        return ((ms + safeFrameMs - 1) / safeFrameMs).coerceAtLeast(1)
    }

    private fun framesToMs(frames: Int): Int {
        return frames.coerceAtLeast(0) * options.frameDurationMs.coerceAtLeast(10)
    }

    private fun stabilizedPlaybackState(current: String, desired: String, now: Long): String {
        if (current == desired) {
            return current
        }
        val minDwellMs = if (options.preferLowLatencyPath) 160L else 280L
        if (now - playbackStateStableSinceMs < minDwellMs) {
            return current
        }
        playbackStateStableSinceMs = now
        return desired
    }

    private fun playbackBackendLabel(): String {
        return playbackSink.backendLabel(options)
    }

    private fun collectWritePayload(
        first: PcmFrame,
        audioQueuedMs: Int,
        jitterBufferedMs: Int,
    ): WriteBatch {
        val bytesPerFrame = first.channels.coerceAtLeast(1) * 2
        fun frameCountFor(payload: ByteArray): Int = (payload.size / bytesPerFrame).coerceAtLeast(1)

        val batchSize = targetWriteBatchSize(
            audioQueuedMs = audioQueuedMs,
            jitterBufferedMs = jitterBufferedMs,
            frameDurationMs = first.frameDurationMs.coerceAtLeast(options.frameDurationMs),
        )
        if (batchSize == 1) {
            return WriteBatch(
                payload = first.payload,
                packetCount = 1,
                pcmFrames = frameCountFor(first.payload),
            )
        }
        val chunks = ArrayList<ByteArray>(batchSize)
        chunks.add(first.payload)
        repeat(batchSize - 1) {
            val next = jitterBuffer.popContiguousForBatch() ?: return@repeat
            if (next.sampleRate == first.sampleRate && next.channels == first.channels) {
                chunks.add(next.payload)
            }
        }
        if (chunks.size == 1) {
            return WriteBatch(
                payload = first.payload,
                packetCount = 1,
                pcmFrames = frameCountFor(first.payload),
            )
        }
        val total = chunks.sumOf { it.size }
        val out = ByteArray(total)
        var offset = 0
        for (chunk in chunks) {
            System.arraycopy(chunk, 0, out, offset, chunk.size)
            offset += chunk.size
        }
        return WriteBatch(
            payload = out,
            packetCount = chunks.size,
            pcmFrames = frameCountFor(out),
        )
    }

    private fun targetWriteBatchSize(
        audioQueuedMs: Int,
        jitterBufferedMs: Int,
        frameDurationMs: Int,
    ): Int {
        val baseBatchSize = options.batchFrames.coerceIn(1, 4)
        if (options.preferLowLatencyPath) {
            balancedQueueBelowFillTarget = false
            return baseBatchSize
        }
        val mode = PlaybackModeProfiles.normalize(stateStore.current().currentAudioMode)
        if (mode != "balanced" && mode != "high_quality") {
            balancedQueueBelowFillTarget = false
            return baseBatchSize
        }
        val fillTargetMs = balancedAudioQueueFillTargetMs(frameDurationMs)
        if (audioQueuedMs >= fillTargetMs) {
            balancedQueueBelowFillTarget = false
            return baseBatchSize
        }
        val availableFrames = ((jitterBufferedMs / frameDurationMs).coerceAtLeast(0) + 1).coerceAtLeast(1)
        if (!balancedQueueBelowFillTarget) {
            balancedQueueBelowFillTarget = true
            val refillTargetMs = if (mode == "high_quality") {
                BALANCED_ONE_SHOT_REFILL_TARGET_MS * 2
            } else {
                BALANCED_ONE_SHOT_REFILL_TARGET_MS
            }
            val refillFramesNeeded = kotlin.math.ceil(
                (refillTargetMs - audioQueuedMs).coerceAtLeast(0) / frameDurationMs.toDouble(),
            ).toInt().coerceAtLeast(1)
            return refillFramesNeeded.coerceAtMost(availableFrames)
        }
        return baseBatchSize.coerceAtMost(availableFrames)
    }

    private fun maybeLogMetrics(force: Boolean = false, reason: String = "periodic") {
        val now = SystemClock.elapsedRealtime()
        val snapshot = stateStore.current()
        val metrics = snapshot.metrics
        val sinkStats = withPlaybackSinkLock { playbackSink.stats() }
        updateSilenceFillAccounting(
            sinkSilenceFillTotal = sinkStats.silenceFillTotal.toInt(),
            nowMs = now,
            playbackState = snapshot.playbackState,
        )
        oboeUnderrunCount = sinkStats.underrunTotal
        if (lastMetricsLogAtMs == 0L) {
            lastMetricsLogAtMs = now
            lastLoggedUdpPackets = metrics.udpPackets
            lastLoggedAudioTrackWriteFrames = metrics.audioTrackWriteFrames
            if (!force) {
                return
            }
        }

        val elapsedMs = (now - lastMetricsLogAtMs).coerceAtLeast(1L)
        if (!force && elapsedMs < 1000L) {
            return
        }

        val observedRxFramesPerSec =
            ((metrics.udpPackets - lastLoggedUdpPackets).coerceAtLeast(0) * 1000.0) / elapsedMs.toDouble()
        val rxFramesPerSec = smoothedRxFramesPerSec?.let { previous ->
            previous + ((observedRxFramesPerSec - previous) * RX_FPS_SMOOTHING_ALPHA)
        } ?: observedRxFramesPerSec
        smoothedRxFramesPerSec = rxFramesPerSec
        val audioTrackWriteFramesPerSec =
            ((metrics.audioTrackWriteFrames - lastLoggedAudioTrackWriteFrames).coerceAtLeast(0L) * 1000.0) /
                elapsedMs.toDouble()
        maybeLogSilenceFillCause(
            nowMs = now,
            totalBufferedMs = metrics.totalBufferedMs,
            audioTrackQueuedMs = metrics.audioTrackQueuedMs,
            rxFramesPerSec = rxFramesPerSec,
        )

        stateStore.update {
            it.copy(
                metrics = it.metrics.copy(
                    rxFramesPerSec = rxFramesPerSec,
                    audioTrackWriteFramesPerSec = audioTrackWriteFramesPerSec,
                    silenceFillCount = silenceFillCount,
                    startupSilenceFillCount = startupSilenceFillCount,
                    cfgChangedCount = cfgChangedCount,
                    discontinuityCount = discontinuityCount,
                    jitterP95Ms = jitterP95Ms,
                floorHoldCount = jitterBuffer.stats.floorHoldCount,
                reconnectCount = reconnectTotalCount,
                decodeErrors = decodeErrorTotal,
                loudnessGainDb = loudnessNormalizer.gainDb(),
            ),
        )
        }

        Log.i(
            logTag,
            String.format(
                Locale.US,
                "playback_summary reason=%s playback=%s total_buffered_ms=%d (jitter=%d+track=%d) audio_track_reported_latency_ms=%s tcp_rtt_ms=%s/%s(med) jitter_p95_ms=%s jitter_underrun=%d floor_hold_count=%d dropped_late_frames=%d/%d silence_fill_count=%d startup_silence_fill_count=%d rx_frames_per_sec=%.1f audio_track_write_frames_per_sec=%.1f cfg_changed=%d discontinuity=%d mode=%s recent=%s",
                reason,
                snapshot.playbackState,
                metrics.totalBufferedMs,
                metrics.jitterBufferedMs,
                metrics.audioTrackQueuedMs,
                metrics.audioTrackLatencyMs?.toString() ?: "null",
                metrics.tcpRoundTripMs?.toString() ?: "null",
                metrics.tcpRoundTripMedianMs?.toString() ?: "null",
                metrics.jitterP95Ms?.toString() ?: "null",
                metrics.jitterUnderrun,
                metrics.floorHoldCount,
                metrics.jitterDropped,
                metrics.jitterLate,
                silenceFillCount,
                startupSilenceFillCount,
                rxFramesPerSec,
                audioTrackWriteFramesPerSec,
                cfgChangedCount,
                discontinuityCount,
                snapshot.currentAudioMode,
                snapshot.recentLog,
            ),
        )

        lastMetricsLogAtMs = now
        lastLoggedUdpPackets = metrics.udpPackets
        lastLoggedAudioTrackWriteFrames = metrics.audioTrackWriteFrames
    }

    private fun maybeLogSilenceFillCause(
        nowMs: Long,
        totalBufferedMs: Int,
        audioTrackQueuedMs: Int,
        rxFramesPerSec: Double,
    ) {
        val silenceDelta = sinkSilenceFillTotal - lastDiagnosedSinkSilenceFillCount
        val underrunDelta = oboeUnderrunCount - lastDiagnosedOboeUnderrunCount
        if (silenceDelta <= 0 && underrunDelta <= 0) {
            return
        }
        val cause = when {
            silenceDelta > 0 && startupSilenceTrackingActive -> "startup_fill"
            silenceDelta > 0 && nowMs - lastLatencyGuardAtMs <= LATENCY_GUARD_CAUSE_WINDOW_MS ->
                "post_latency_guard"
            silenceDelta > 0 &&
                audioTrackQueuedMs <= balancedAudioQueueLowWatermarkMs(options.frameDurationMs) ->
                "buffer_empty"
            underrunDelta > 0 -> "oboe_starvation"
            else -> "unknown"
        }
        Log.w(
            logTag,
            String.format(
                Locale.US,
                "silence_fill_cause cause=%s silence_delta=%d underrun_delta=%d steady_silence_fill=%d startup_silence_fill=%d latency_guard_drop_count=%d total_buffered_ms=%d track_queued_ms=%d rx_frames_per_sec=%.1f recent=%s",
                cause,
                silenceDelta.coerceAtLeast(0),
                underrunDelta.coerceAtLeast(0),
                silenceFillCount,
                startupSilenceFillCount,
                latencyGuardDropCount,
                totalBufferedMs,
                audioTrackQueuedMs,
                rxFramesPerSec,
                stateStore.current().recentLog,
            ),
        )
        lastDiagnosedSilenceFillCount = silenceFillCount
        lastDiagnosedSinkSilenceFillCount = sinkSilenceFillTotal
        lastDiagnosedOboeUnderrunCount = oboeUnderrunCount
    }

    private fun updateSilenceFillAccounting(
        sinkSilenceFillTotal: Int,
        nowMs: Long,
        playbackState: String? = null,
    ) {
        this.sinkSilenceFillTotal = sinkSilenceFillTotal.coerceAtLeast(0)
        val effectivePlaybackState = playbackState ?: stateStore.current().playbackState
        if (startupSilenceTrackingActive &&
            startupSilencePhaseUntilMs == 0L &&
            effectivePlaybackState == "playing"
        ) {
            startupSilencePhaseUntilMs = nowMs + STARTUP_SILENCE_PHASE_MS
        }
        if (startupSilenceTrackingActive &&
            startupSilencePhaseUntilMs > 0L &&
            nowMs >= startupSilencePhaseUntilMs
        ) {
            startupSilenceFillBaseline = this.sinkSilenceFillTotal
            startupSilenceTrackingActive = false
        }
        if (startupSilenceTrackingActive) {
            startupSilenceFillCount = this.sinkSilenceFillTotal
            silenceFillCount = 0
            return
        }
        startupSilenceFillCount = startupSilenceFillBaseline.coerceAtMost(this.sinkSilenceFillTotal)
        silenceFillCount = (this.sinkSilenceFillTotal - startupSilenceFillCount).coerceAtLeast(0)
    }

    companion object {
        private const val MAX_AUTO_RECONNECT_ATTEMPTS = 5
        private const val DECODE_SUMMARY_INTERVAL_MS = 5_000L
        private const val JITTER_P95_WINDOW_FRAMES = 50
        private const val LOW_LATENCY_ADAPTIVE_BOOST_MS = 5
        private const val LOW_LATENCY_RECOVER_MS = 10_000L
        private const val WIFI_BALANCED_MAX_BUFFER_CAP_MS = 30
        private const val BALANCED_PACING_SLOWDOWN_MAX_MS = 4
        private const val LATENCY_GUARD_OBSERVE_WINDOW_MS = 250L
        private const val LATENCY_GUARD_CAUSE_WINDOW_MS = 800L
        private const val BALANCED_AUDIO_QUEUE_LOW_WATERMARK_EXTRA_MS = 10
        private const val BALANCED_AUDIO_QUEUE_FILL_TARGET_EXTRA_MS = 10
        private const val BALANCED_ONE_SHOT_REFILL_TARGET_MS = 50
        private const val STARTUP_SILENCE_PHASE_MS = 3_000L
        private const val BUFFER_EMPTY_LOW_WATERMARK_MS = 20
        private const val BALANCED_EXPECTED_RX_FRAMES_PER_SEC = 50.0
        private const val BALANCED_RX_FRAMES_FLOOR = 48.0
        private const val RX_FPS_SMOOTHING_ALPHA = 0.25
        private const val HIGH_QUALITY_PREFILL_MAX_WAIT_MS = 500L
    }

    private fun reconnectDelayMs(attemptNumber: Int): Long {
        val shift = (attemptNumber - 1).coerceIn(0, 4)
        return 1_000L shl shift
    }

    private data class WriteBatch(
        val payload: ByteArray,
        val packetCount: Int,
        val pcmFrames: Int,
    )

    private data class ActiveAudioFormat(
        val sampleRate: Int,
        val channels: Int,
        val frameSamplesPerChannel: Int,
    )

    private data class AudioBackendConfig(
        val transportHint: TransportHint,
        val preferLowLatencyPath: Boolean,
        val lowLatencyBufferMultiplier: Int,
        val lowLatencyFallbackBufferMultiplier: Int,
        val preferredBackend: String,
    )

    private data class DecodeResult(
        val packet: LasPacket?,
        val decodeMs: Float,
    )
}
