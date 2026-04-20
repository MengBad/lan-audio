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
import java.net.DatagramPacket
import java.net.DatagramSocket
import java.net.SocketTimeoutException
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledExecutorService
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

class StreamSessionManager(
    private val target: PlaybackTarget,
    private val pingIntervalMs: Int,
    private val callback: Callback,
) {
    private val logTag = "lan_audio_stream"
    private val clientAppVersion = "android_flutter"
    interface Callback {
        fun onLog(message: String)
        fun onWsConnected()
        fun onWsDisconnected(reason: String)
        fun onUdpPacket(packet: LasPacket)
        fun onControlHelloAck(protocolVersion: Int, currentAudioMode: String, capabilities: Map<String, Boolean>)
        fun onServerInfo(platform: String?, appVersion: String?, currentAudioMode: String)
        fun onAudioModeChanged(mode: String, applied: Boolean, reason: String)
        fun onError(code: String, message: String)
    }

    private val okHttpClient = OkHttpClient.Builder().build()
    private var webSocket: WebSocket? = null
    private var udpSocket: DatagramSocket? = null
    private var udpThread: Thread? = null
    private var pingExecutor: ScheduledExecutorService? = null
    private val pingSeq = AtomicInteger(0)
    private var wsReady = false
    private var currentAudioMode: String = "balanced"

    @Volatile
    private var running = false

    fun start() {
        if (running) {
            return
        }
        running = true
        Log.i(logTag, "start host=${target.host} ws=${target.wsPort} udp=${target.udpPort}")
        startUdpReceiver()
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
        okHttpClient.dispatcher.executorService.shutdown()
        okHttpClient.connectionPool.evictAll()
        wsReady = false
    }

    fun setAudioMode(mode: String, reason: String = "user_request"): Boolean {
        if (!running || !wsReady) {
            return false
        }
        val setMode = JSONObject(
            mapOf(
                "type" to "set_audio_mode",
                "mode" to mode,
                "reason" to reason,
            ),
        )
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

    private fun startWebSocket() {
        val request = Request.Builder()
            .url("ws://${target.host}:${target.wsPort}/")
            .build()
        webSocket = okHttpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    Log.i(logTag, "ws open")
                    val localUdpPort = udpSocket?.localPort ?: 0
                    val platformOpusDecoderAvailable = OpusFrameDecoder.isAvailable()
                    // Opus remains opt-in and experimental, but can be negotiated whenever
                    // the bundled libopus/JNI decoder is available. PCM16 remains default.
                    val supportsVerifiedOpusPlayback = platformOpusDecoderAvailable
                    val hello = JSONObject(
                        mapOf(
                            "type" to "hello",
                            "protocol_version" to 2,
                            "device_name" to "${Build.MANUFACTURER} ${Build.MODEL}".trim(),
                            "client_id" to "android-${System.currentTimeMillis()}",
                            "udp_port" to localUdpPort,
                            "desired_sample_rate" to 48000,
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
                    if (!running) {
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
                                callback.onControlHelloAck(protocolVersion, currentAudioMode, capabilities)
                                if (!wsReady) {
                                    wsReady = true
                                    callback.onWsConnected()
                                }
                                callback.onLog("v2_hello_ack")
                            }
                            "server_info" -> {
                                val mode = msg.optString("current_audio_mode", currentAudioMode)
                                currentAudioMode = mode
                                callback.onServerInfo(
                                    msg.optString("platform").ifBlank { null },
                                    msg.optString("app_version").ifBlank { null },
                                    mode,
                                )
                                callback.onLog("v2_server_info")
                            }
                            "audio_mode_changed" -> {
                                val mode = msg.optString("mode", currentAudioMode)
                                val applied = msg.optBoolean("applied", false)
                                val reason = msg.optString("reason", "")
                                if (applied) {
                                    currentAudioMode = mode
                                }
                                callback.onAudioModeChanged(mode, applied, reason)
                                callback.onLog("v2_audio_mode_changed:$mode")
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
                    if (!running) {
                        return
                    }
                    Log.e(logTag, "ws failure: ${t.message}")
                    wsReady = false
                    callback.onLog("ws_failure:${t.message ?: "unknown"}")
                    callback.onWsDisconnected("ws_failure")
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (!running) {
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
                webSocket?.send(ping.toString())
            } catch (t: Throwable) {
                Log.e(logTag, "ws ping failed: ${t.message}")
                wsReady = false
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
}
