package com.example.lan_audio_android_mvp

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioManager
import android.os.Build
import android.util.Log
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit

class PlaybackSessionController(
    private val context: Context,
    private val stateStore: PlaybackStateStore,
) {
    private val logTag = "lan_audio_session"
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val audioTrackController = AudioTrackController()
    private val controlExecutor: ScheduledExecutorService = Executors.newSingleThreadScheduledExecutor()
    private var playoutFuture: ScheduledFuture<*>? = null
    private var reconnectFuture: ScheduledFuture<*>? = null
    private var streamManager: StreamSessionManager? = null
    private var jitterBuffer = PlaybackJitterBuffer(60, 300)
    private var options = PlaybackOptions()
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
        reconnectFuture?.cancel(false)
        reconnectFuture = null
        stopStreamAndAudio()
        val generation = ++streamGeneration

        if (!requestAudioFocus()) {
            Log.w(logTag, "audio focus denied")
            publishError("audio_focus_denied", "audio focus denied")
            return
        }
        registerNoisyReceiver()

        jitterBuffer = PlaybackJitterBuffer(options.startBufferMs, options.maxBufferMs)
        lastSeq = null
        udpLoss = 0

        stateStore.update {
            it.copy(
                serviceState = "running",
                connectionState = "connecting",
                playbackState = "buffering",
                targetHost = target.host,
                targetName = target.serverName,
                recentLog = "connecting:${target.serverName}(${target.host})",
                error = null,
                metrics = it.metrics.copy(
                    bufferedMs = 0,
                    jitterUnderrun = 0,
                    jitterDropped = 0,
                    jitterLate = 0,
                    udpPackets = 0,
                    udpBytes = 0,
                    lossEstimate = 0,
                    lastSeq = null,
                ),
            )
        }

        streamManager = StreamSessionManager(target, options.pingIntervalMs, callback = object :
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
                stateStore.update {
                    it.copy(
                        connectionState = "connected",
                        playbackState = "buffering",
                        recentLog = "ws_connected",
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

            override fun onError(code: String, message: String) {
                if (generation != streamGeneration) return
                Log.e(logTag, "stream error code=$code message=$message")
                publishError(code, message)
                scheduleReconnect(code)
            }
        })
        streamManager?.start()
    }

    fun stopPlayback(reason: String = "user_stop") {
        Log.i(logTag, "stopPlayback reason=$reason")
        reconnectFuture?.cancel(false)
        reconnectFuture = null
        stateStore.update {
            it.copy(
                serviceState = "stopping",
                connectionState = "disconnected",
                playbackState = "stopped",
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
                recentLog = reason,
            )
        }
    }

    fun reconnect(reason: String = "manual_reconnect") {
        Log.i(logTag, "reconnect reason=$reason")
        scheduleReconnect(reason, immediate = true)
    }

    fun setOptions(next: PlaybackOptions) {
        options = next
        jitterBuffer = PlaybackJitterBuffer(options.startBufferMs, options.maxBufferMs)
        stateStore.update { it.copy(recentLog = "options_updated") }
    }

    fun destroy() {
        reconnectFuture?.cancel(true)
        controlExecutor.shutdownNow()
        stopPlayback("controller_destroy")
    }

    private fun ensurePlayoutLoop() {
        if (playoutFuture != null && !playoutFuture!!.isCancelled) {
            return
        }
        playoutFuture = controlExecutor.scheduleAtFixedRate({
            try {
                playoutTick()
            } catch (t: Throwable) {
                publishError("playout_tick_failed", t.message ?: "playout failed")
            }
        }, 10L, 10L, TimeUnit.MILLISECONDS)
    }

    private fun handleUdpPacket(packet: LasPacket) {
        if (lastSeq != null) {
            val expected = ((lastSeq!! + 1L) and 0xFFFFFFFFL).toInt()
            if (packet.sequence != expected) {
                udpLoss += ((packet.sequence.toLong() - expected.toLong()) and 0xFFFFFFFFL).toInt()
            }
        }
        lastSeq = packet.sequence.toLong() and 0xFFFFFFFFL
        jitterBuffer.push(packet)

        val stats = jitterBuffer.stats
        stateStore.update {
            val current = it.metrics
            it.copy(
                metrics = current.copy(
                    sampleRate = packet.sampleRate,
                    channels = packet.channels,
                    bufferedMs = jitterBuffer.bufferedMs(),
                    jitterUnderrun = stats.underrunCount,
                    jitterDropped = stats.droppedFrames,
                    jitterLate = stats.lateFrames,
                    udpPackets = current.udpPackets + 1,
                    udpBytes = current.udpBytes + packet.payload.size + 27,
                    lossEstimate = udpLoss,
                    lastSeq = lastSeq,
                ),
            )
        }
    }

    private fun playoutTick() {
        val frame = jitterBuffer.pop()
        val stats = jitterBuffer.stats
        if (frame == null) {
            stateStore.update {
                it.copy(
                    playbackState = "buffering",
                    metrics = it.metrics.copy(
                        bufferedMs = jitterBuffer.bufferedMs(),
                        jitterUnderrun = stats.underrunCount,
                        jitterDropped = stats.droppedFrames,
                        jitterLate = stats.lateFrames,
                    ),
                )
            }
            return
        }

        if (!audioInit) {
            Log.i(logTag, "init AudioTrack sr=${frame.sampleRate} ch=${frame.channels}")
            audioTrackController.init(
                sampleRate = frame.sampleRate,
                channels = frame.channels,
                frameSamplesPerChannel = frame.frameDurationMs * frame.sampleRate / 1000,
            )
            audioInit = true
        }
        if (!audioStarted) {
            audioTrackController.start()
            audioStarted = true
        }
        audioTrackController.writePcm16(frame.payload)
        val audioStats = audioTrackController.stats()
        stateStore.update {
            it.copy(
                playbackState = "playing",
                metrics = it.metrics.copy(
                    sampleRate = frame.sampleRate,
                    channels = frame.channels,
                    bufferedMs = jitterBuffer.bufferedMs(),
                    jitterUnderrun = stats.underrunCount,
                    jitterDropped = stats.droppedFrames,
                    jitterLate = stats.lateFrames,
                    nativeQueuedFrames = audioStats.nativeQueuedFrames,
                    audioTrackWriteFrames = audioStats.audioTrackWriteFrames,
                    audioTrackShortWriteCount = audioStats.audioTrackShortWriteCount,
                ),
            )
        }
    }

    private fun scheduleReconnect(reason: String, immediate: Boolean = false) {
        val target = currentTarget ?: return
        Log.w(logTag, "scheduleReconnect reason=$reason immediate=$immediate")
        reconnectFuture?.cancel(false)
        val generation = ++streamGeneration
        reconnectFuture = controlExecutor.schedule({
            if (generation != streamGeneration) {
                return@schedule
            }
            stateStore.update {
                it.copy(
                    serviceState = "running",
                    connectionState = "reconnecting",
                    playbackState = "buffering",
                    recentLog = "reconnect:$reason",
                )
            }
            streamManager?.stop()
            streamManager = null
            audioTrackController.stop()
            audioTrackController.release()
            audioInit = false
            audioStarted = false
            jitterBuffer.clear()
            streamManager = StreamSessionManager(target, options.pingIntervalMs, object : StreamSessionManager.Callback {
                override fun onLog(message: String) {
                    if (generation != streamGeneration) return
                    stateStore.update { it.copy(recentLog = message) }
                }

                override fun onWsConnected() {
                    if (generation != streamGeneration) return
                    reconnectFuture?.cancel(false)
                    reconnectFuture = null
                    stateStore.update {
                        it.copy(connectionState = "connected", playbackState = "buffering", recentLog = "ws_reconnected")
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

                override fun onError(code: String, message: String) {
                    if (generation != streamGeneration) return
                    publishError(code, message)
                    scheduleReconnect(code)
                }
            })
            streamManager?.start()
            ensurePlayoutLoop()
        }, if (immediate) 0L else 2000L, TimeUnit.MILLISECONDS)
    }

    private fun stopStreamAndAudio() {
        streamGeneration += 1
        streamManager?.stop()
        streamManager = null
        playoutFuture?.cancel(true)
        playoutFuture = null
        audioTrackController.stop()
        audioTrackController.release()
        audioInit = false
        audioStarted = false
        jitterBuffer.clear()
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
}
