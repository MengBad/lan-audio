package com.example.lan_audio_android_mvp

import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import android.util.Log
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
    interface Callback {
        fun onLog(message: String)
        fun onWsConnected()
        fun onWsDisconnected(reason: String)
        fun onUdpPacket(packet: LasPacket)
        fun onError(code: String, message: String)
    }

    private val okHttpClient = OkHttpClient.Builder().build()
    private var webSocket: WebSocket? = null
    private var udpSocket: DatagramSocket? = null
    private var udpThread: Thread? = null
    private var pingExecutor: ScheduledExecutorService? = null
    private val pingSeq = AtomicInteger(0)

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
        Log.i(logTag, "stop")
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
    }

    private fun startUdpReceiver() {
        val socket = DatagramSocket(0)
        socket.soTimeout = 1000
        udpSocket = socket
        Log.i(logTag, "udp bound localPort=${socket.localPort}")
        callback.onLog("udp_bound:${socket.localPort}")
        udpThread = Thread({
            val buffer = ByteArray(8192)
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
                    val hello = JSONObject(
                        mapOf(
                            "type" to "client_hello",
                            "client_name" to "android-service",
                            "udp_port" to localUdpPort,
                            "desired_sample_rate" to 48000,
                            "channels" to 2,
                        ),
                    )
                    webSocket.send(hello.toString())
                    callback.onWsConnected()
                    callback.onLog("ws_connected")
                    startPingLoop()
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (!running) {
                        return
                    }
                    Log.e(logTag, "ws failure: ${t.message}")
                    callback.onError("ws_failure", t.message ?: "ws failure")
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    if (!running) {
                        return
                    }
                    Log.w(logTag, "ws closed code=$code reason=$reason")
                    callback.onLog("ws_closed:$code:$reason")
                    callback.onWsDisconnected("ws_closed")
                }
            },
        )
    }

    private fun startPingLoop() {
        pingExecutor?.shutdownNow()
        pingExecutor = Executors.newSingleThreadScheduledExecutor()
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
                callback.onError("ws_ping_failed", t.message ?: "ping failed")
            }
        }, 1000L, pingIntervalMs.toLong().coerceAtLeast(250L), TimeUnit.MILLISECONDS)
    }
}
