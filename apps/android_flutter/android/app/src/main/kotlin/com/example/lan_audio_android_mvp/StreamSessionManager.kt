package com.example.lan_audio_android_mvp

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import android.os.Process
import android.util.Log
import android.os.Build
import org.json.JSONObject
import java.io.BufferedInputStream
import java.io.DataInputStream
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.InetSocketAddress
import java.net.Socket
import java.net.SocketTimeoutException
import java.util.ArrayDeque
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class StreamSessionManager(
    private val target: PlaybackTarget,
    private val pingIntervalMs: Int,
    initialAudioMode: String = "balanced",
    private val callback: Callback,
) {
    private val logTag = "lan_audio_stream"
    private val clientAppVersion = "android_flutter"
    interface Callback {
        fun onLog(message: String)
        fun onWsConnected()
        fun onWsDisconnected(reason: String)
        fun onUdpPacket(packet: LasPacket)
        fun onControlHelloAck(
            protocolVersion: Int,
            currentAudioMode: String,
            capabilities: Map<String, Boolean>,
            transportType: String,
        )
        fun onServerInfo(
            platform: String?,
            appVersion: String?,
            currentAudioMode: String,
            mixFormatHz: Int? = null,
        )
        fun onAudioModeChanged(
            mode: String,
            applied: Boolean,
            reason: String,
            effectiveCodec: String? = null,
        )
        fun onClientCountUpdated(count: Int)
        fun onTcpRoundTripMs(roundTripMs: Int, medianMs: Int)
        fun onError(code: String, message: String)

        /**
         * Phase 3: provide the latest playback watermark so it can be sent
         * up to the server's adaptive sync engine. Default returns null,
         * which keeps the previous (no-feedback) behaviour for any caller
         * that has not been migrated yet.
         */
        fun provideWatermark(): WatermarkSample? = null
    }

    /** Snapshot of client buffer/jitter state used by Phase 3 reports. */
    data class WatermarkSample(
        val jitterBufMs: Int,
        val ringBufMs: Int,
        val silenceFillDelta: Int,
        val underrunDelta: Int,
        val jitterP95Us: Int,
    )

    private val okHttpClient = OkHttpClient.Builder().build()
    private var webSocket: WebSocket? = null
    private var udpSocket: DatagramSocket? = null
    private var udpThread: Thread? = null
    private var tcpSocket: Socket? = null
    private var tcpThread: Thread? = null
    private var pingExecutor: ScheduledExecutorService? = null
    private val pingSeq = AtomicInteger(0)
    private val pingSentAt = ConcurrentHashMap<Int, Long>()
    private var wsReady = false
    private var currentAudioMode: String = PlaybackModeProfiles.normalize(initialAudioMode)
    private var connectedClients: Int = 0
    private val recentRttMs = ArrayDeque<Int>()
    private val preferredSampleRate: Int = AudioTrackController.preferredStreamSampleRate()

    @Volatile
    private var running = false

    fun start() {
        if (running) {
            return
        }
        running = true
        Log.i(logTag, "start host=${target.host} ws=${target.wsPort} udp=${target.udpPort}")
        if (target.transportMode == "usb") {
            startTcpReceiver()
        } else {
            startUdpReceiver()
        }
        startWebSocket()
    }

    fun stop() {
        Log.i(
            logTag,
            "stop wsReady=$wsReady udpLocalPort=${udpSocket?.localPort ?: -1}"
        )
        running = false
        pingExecutor?.shutdownNow()
        pingExecutor = null
        webSocket?.close(1000, "stop")
        webSocket = null
        udpSocket?.close()
        udpSocket = null
        udpThread?.interrupt()
        udpThread = null
        tcpSocket?.close()
        tcpSocket = null
        tcpThread?.interrupt()
        tcpThread = null
        okHttpClient.dispatcher.executorService.shutdown()
        okHttpClient.connectionPool.evictAll()
        wsReady = false
    }

    fun setAudioMode(
        mode: String,
        reason: String = "user_request",
        preferredCodec: String? = null,
    ): Boolean {
        if (!running || !wsReady) {
            return false
        }
        val payload = mutableMapOf<String, Any>(
            "type" to "set_audio_mode",
            "mode" to mode,
            "reason" to reason,
            "preferred_sample_rate" to preferredSampleRate,
        )
        // Phase 7: codec selector. The server accepts `pcm16` / `opus` and
        // ignores the field if absent — so older servers stay compatible.
        if (preferredCodec != null) {
            payload["preferred_codec"] = preferredCodec
        }
        val setMode = JSONObject(payload as Map<String, Any?>)
        return webSocket?.send(setMode.toString()) ?: false
    }

    private fun startUdpReceiver() {
        val socket = DatagramSocket(0)
        socket.soTimeout = 1000
        udpSocket = socket
        Log.i(logTag, "udp receiver started localPort=${socket.localPort}")
        callback.onLog("udp_bound:${socket.localPort}")
        udpThread = Thread({
            val threadName = Thread.currentThread().name
            val threadId = Process.myTid()
            try {
                Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
                val effectivePriority = Process.getThreadPriority(threadId)
                Log.i(
                    logTag,
                    "udp receiver thread priority set name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) effective=$effectivePriority",
                )
            } catch (t: Throwable) {
                Log.w(
                    logTag,
                    "udp receiver thread priority set failed name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) error=${t.message}",
                )
            }
            val buffer = ByteArray(8192)
            try {
                while (running) {
                    try {
                        val packet = DatagramPacket(buffer, buffer.size)
                        socket.receive(packet)
                        val raw = packet.data.copyOfRange(packet.offset, packet.offset + packet.length)
                        val las = LasPacketParser.parse(raw) ?: continue
                        callback.onUdpPacket(las)
                    } catch (_: SocketTimeoutException) {
                        continue
                    } catch (_: Throwable) {
                        if (running) {
                            Log.e(logTag, "udp receive failed")
                            callback.onError("udp_receive_failed", "udp receive failed")
                        }
                    }
                }
            } finally {
                Log.i(logTag, "udp receiver stopped localPort=${socket.localPort}")
            }
        }, "lan-audio-service-udp").also { it.start() }
    }

    private fun startTcpReceiver() {
        Log.i(logTag, "tcp receiver start host=${target.host} port=${target.udpPort}")
        callback.onLog("tcp_connect:${target.host}:${target.udpPort}")
        tcpThread = Thread({
            val threadName = Thread.currentThread().name
            val threadId = Process.myTid()
            try {
                Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
                val effectivePriority = Process.getThreadPriority(threadId)
                Log.i(
                    logTag,
                    "tcp receiver thread priority set name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) effective=$effectivePriority",
                )
            } catch (t: Throwable) {
                Log.w(
                    logTag,
                    "tcp receiver thread priority set failed name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) error=${t.message}",
                )
            }
            try {
                val socket = Socket()
                socket.tcpNoDelay = true
                socket.receiveBufferSize = USB_DIRECT_RECEIVE_BUFFER_BYTES
                socket.connect(InetSocketAddress(target.host, target.udpPort), USB_DIRECT_CONNECT_TIMEOUT_MS)
                socket.soTimeout = 1000
                tcpSocket = socket
                DataInputStream(BufferedInputStream(socket.getInputStream())).use { input ->
                    while (running) {
                        try {
                            val frameLen = input.readInt()
                            if (frameLen <= 0 || frameLen > 256 * 1024) {
                                callback.onError("tcp_frame_invalid", "invalid tcp frame length=$frameLen")
                                continue
                            }
                            val raw = ByteArray(frameLen)
                            input.readFully(raw)
                            val las = LasPacketParser.parse(raw) ?: continue
                            callback.onUdpPacket(las)
                        } catch (_: SocketTimeoutException) {
                            continue
                        }
                    }
                }
            } catch (t: Throwable) {
                if (running) {
                    Log.e(logTag, "tcp receive failed: ${t.message}")
                    callback.onError("tcp_receive_failed", "tcp receive failed: ${t.message}")
                }
            } finally {
                Log.i(logTag, "tcp receiver stopped")
            }
        }, "lan-audio-service-tcp").also { it.start() }
    }

    private fun startWebSocket() {
        val request = Request.Builder()
            .url("ws://${target.host}:${target.wsPort}/")
            .build()
        webSocket = okHttpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    if (!running || this@StreamSessionManager.webSocket !== webSocket) {
                        webSocket.close(1000, "stale_session")
                        return
                    }
                    Log.i(logTag, "ws open")
                    val localUdpPort = if (target.transportMode == "usb") 0 else (udpSocket?.localPort ?: 0)
                    val platformOpusDecoderAvailable = OpusFrameDecoder.isAvailable()
                    // Opus remains opt-in and experimental, but can be negotiated whenever
                    // the bundled libopus/JNI decoder is available. PCM16 remains default.
                    val supportsVerifiedOpusPlayback = platformOpusDecoderAvailable
                    val hello = JSONObject(
                        mapOf(
                            "type" to "hello",
                            "protocol_version" to 2,
                            "device_name" to "${Build.MANUFACTURER} ${Build.MODEL}".trim(),
                            "client_id" to "android-${Build.MANUFACTURER}-${Build.MODEL}".trim().replace(" ", "_"),
                            "udp_port" to localUdpPort,
                            "desired_sample_rate" to preferredSampleRate,
                            "channels" to 2,
                            "preferred_audio_mode" to currentAudioMode,
                            "capabilities" to mapOf(
                                "supports_pcm16" to true,
                                "supports_f32" to false,
                                "supports_modes" to true,
                                "supports_metrics" to true,
                                "supports_opus_future" to platformOpusDecoderAvailable,
                                "supports_opus" to supportsVerifiedOpusPlayback,
                                "supports_opus_experimental" to supportsVerifiedOpusPlayback,
                                "supports_low_latency" to true,
                                "supports_high_quality" to true,
                                "supports_native_audio_track" to true,
                                "supports_fast_path" to true,
                                "supports_stable_audio_track" to true,
                                "supports_usb_tethering" to true,
                                "supports_usb_direct_future" to false,
                                // Phase 6.4 Hi-Res. We can decode v3 PCM24
                                // packets and route them through the
                                // float-aware Oboe sink (which preserves
                                // the full 24-bit dynamic range). On
                                // pre-O_MR1 devices the legacy
                                // AudioTrackController is used and we
                                // still degrade to PCM16 in the runtime.
                                "supports_hires_pcm24" to true,
                            ),
                        ),
                    )
                    webSocket.send(hello.toString())

                    val clientInfo = JSONObject(
                        mapOf(
                            "type" to "client_info",
                            "client_name" to "android-service",
                            "platform" to "android",
                            "app_version" to clientAppVersion,
                            "udp_port" to localUdpPort,
                        ),
                    )
                    webSocket.send(clientInfo.toString())
                    callback.onLog("v2_hello_sent")
                    startPingLoop()
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    if (!running || this@StreamSessionManager.webSocket !== webSocket) {
                        return
                    }
                    try {
                        val msg = JSONObject(text)
                        when (msg.optString("type")) {
                            "hello_ack" -> {
                                val protocolVersion = msg.optInt("protocol_version", 0)
                                val accepted = msg.optBoolean("accepted", false)
                                if (!accepted) {
                                    callback.onError("hello_rejected", msg.optString("message", "hello rejected"))
                                    return
                                }
                                currentAudioMode = msg.optString("current_audio_mode", currentAudioMode)
                                val capabilitiesJson = msg.optJSONObject("capabilities")
                                val capabilities = jsonObjectToBooleanMap(capabilitiesJson)
                                val transportType = msg.optString(
                                    "transport_type",
                                    if (target.transportMode == "usb") "usb" else "wifi",
                                ).lowercase()
                                callback.onControlHelloAck(
                                    protocolVersion,
                                    currentAudioMode,
                                    capabilities,
                                    transportType,
                                )
                                if (!wsReady) {
                                    wsReady = true
                                    callback.onWsConnected()
                                }
                                callback.onLog("v2_hello_ack")
                            }
                            "server_info" -> {
                                val mode = msg.optString("current_audio_mode", currentAudioMode)
                                currentAudioMode = mode
                                val mixFormatHz = if (msg.has("mix_format_hz") && !msg.isNull("mix_format_hz")) {
                                    msg.optInt("mix_format_hz", 0).takeIf { it > 0 }
                                } else {
                                    null
                                }
                                callback.onServerInfo(
                                    msg.optString("platform").ifBlank { null },
                                    msg.optString("app_version").ifBlank { null },
                                    mode,
                                    mixFormatHz,
                                )
                                callback.onLog("v2_server_info")
                            }
                            "server_pong" -> {
                                val seq = msg.optInt("seq", -1)
                                if (seq >= 0) {
                                    val sentAt = pingSentAt.remove(seq)
                                    if (sentAt != null) {
                                        val rttMs = (System.currentTimeMillis() - sentAt).coerceAtLeast(0)
                                        val currentRtt = rttMs.toInt()
                                        val median = updateRttMedian(currentRtt)
                                        if (median > 0 && currentRtt > median * 10) {
                                            Log.w(
                                                logTag,
                                                "tcp_rtt_spike current=${currentRtt}ms median=${median}ms transport=${target.transportMode}",
                                            )
                                            callback.onLog("tcp_rtt_spike:${currentRtt}ms/${median}ms")
                                        }
                                        callback.onTcpRoundTripMs(currentRtt, median)
                                    }
                                }
                            }
                            "audio_mode_changed" -> {
                                val mode = msg.optString("mode", currentAudioMode)
                                val applied = msg.optBoolean("applied", false)
                                val reason = msg.optString("reason", "")
                                val effectiveCodec = msg
                                    .optString("effective_codec", "")
                                    .takeIf { it.isNotEmpty() }
                                if (applied) {
                                    currentAudioMode = mode
                                }
                                callback.onAudioModeChanged(mode, applied, reason, effectiveCodec)
                                callback.onLog("v2_audio_mode_changed:$mode")
                            }
                            "client_list" -> {
                                val clients = msg.optJSONArray("clients")
                                connectedClients = clients?.length() ?: 0
                                callback.onClientCountUpdated(connectedClients)
                            }
                            "client_joined" -> {
                                connectedClients += 1
                                callback.onClientCountUpdated(connectedClients)
                            }
                            "client_left" -> {
                                connectedClients = (connectedClients - 1).coerceAtLeast(0)
                                callback.onClientCountUpdated(connectedClients)
                            }
                            "error" -> {
                                callback.onError(
                                    msg.optString("code", "protocol_error"),
                                    msg.optString("message", "unknown protocol error"),
                                )
                            }
                            "server_welcome" -> {
                                // Legacy fallback path: keep compatibility with pre-v2 server.
                                if (!wsReady) {
                                    wsReady = true
                                    callback.onWsConnected()
                                }
                            }
                            else -> {
                                // no-op for unknown or metrics/pong messages
                            }
                        }
                    } catch (t: Throwable) {
                        Log.w(logTag, "ws onMessage parse failed: ${t.message}")
                        callback.onLog("ws_message_parse_failed")
                    }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (!running || this@StreamSessionManager.webSocket !== webSocket) {
                        return
                    }
                    Log.e(logTag, "ws failure: ${t.message}")
                    wsReady = false
                    callback.onLog("ws_failure:${t.message ?: "unknown"}")
                    callback.onWsDisconnected("ws_failure")
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (!running || this@StreamSessionManager.webSocket !== webSocket) {
                        return
                    }
                    Log.w(logTag, "ws closed code=$code reason=$reason")
                    callback.onLog("ws_closed:$code:$reason")
                    wsReady = false
                    callback.onWsDisconnected("ws_closed")
                }
            },
        )
    }

    private fun startPingLoop() {
        pingExecutor?.shutdownNow()
        pingExecutor = Executors.newSingleThreadScheduledExecutor()
        Log.i(logTag, "ws ping loop started intervalMs=${pingIntervalMs.coerceAtLeast(250)}")
        pingExecutor?.scheduleAtFixedRate({
            if (!running) {
                return@scheduleAtFixedRate
            }
            try {
                val ping = JSONObject(
                    mapOf(
                        "type" to "client_ping",
                        "seq" to pingSeq.getAndIncrement(),
                        "ts_unix_ms" to System.currentTimeMillis(),
                    ),
                )
                val seq = ping.optInt("seq", -1)
                if (seq >= 0) {
                    pingSentAt[seq] = System.currentTimeMillis()
                }
                webSocket?.send(ping.toString())

                // Phase 3 watermark report — piggybacks on the same ping
                // cadence to feed the server's Kalman+PID sync engine.
                try {
                    val sample = callback.provideWatermark()
                    if (sample != null) {
                        val watermark = JSONObject(
                            mapOf(
                                "type" to "client_watermark",
                                "ts_unix_ms" to System.currentTimeMillis(),
                                "jitter_buf_ms" to sample.jitterBufMs.coerceAtLeast(0),
                                "ring_buf_ms" to sample.ringBufMs.coerceAtLeast(0),
                                "silence_fill_delta" to sample.silenceFillDelta.coerceAtLeast(0),
                                "underrun_delta" to sample.underrunDelta.coerceAtLeast(0),
                                "jitter_p95_us" to sample.jitterP95Us.coerceAtLeast(0),
                            ),
                        )
                        webSocket?.send(watermark.toString())
                    }
                } catch (t: Throwable) {
                    // Watermark reporting is best-effort — never tear down
                    // the session because the report failed.
                    Log.w(logTag, "watermark report failed: ${t.message}")
                }
            } catch (t: Throwable) {
                Log.e(logTag, "ws ping failed: ${t.message}")
                wsReady = false
                try { webSocket?.close(1001, "ping_failed") } catch (_: Throwable) {}
                try { webSocket?.cancel() } catch (_: Throwable) {}
                callback.onWsDisconnected("ws_ping_failed")
            }
        }, 1000L, pingIntervalMs.toLong().coerceAtLeast(250L), TimeUnit.MILLISECONDS)
    }

    private fun jsonObjectToBooleanMap(json: JSONObject?): Map<String, Boolean> {
        if (json == null) {
            return emptyMap()
        }
        val out = mutableMapOf<String, Boolean>()
        val it = json.keys()
        while (it.hasNext()) {
            val key = it.next()
            out[key] = json.optBoolean(key, false)
        }
        return out
    }

    private fun updateRttMedian(sampleMs: Int): Int {
        if (recentRttMs.size >= 20) {
            recentRttMs.removeFirst()
        }
        recentRttMs.addLast(sampleMs.coerceAtLeast(0))
        if (recentRttMs.isEmpty()) {
            return 0
        }
        val sorted = recentRttMs.toMutableList().sorted()
        return sorted[sorted.size / 2]
    }

    companion object {
        private const val USB_DIRECT_CONNECT_TIMEOUT_MS = 3000
        private const val USB_DIRECT_RECEIVE_BUFFER_BYTES = 256 * 1024
    }
}
