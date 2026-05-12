package com.example.lan_audio_android_mvp

import android.content.Context
import android.media.AudioManager
import android.util.Log
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import java.io.BufferedReader
import java.io.InputStreamReader
import java.io.OutputStreamWriter
import java.net.Socket
import java.util.concurrent.atomic.AtomicBoolean
import org.json.JSONObject

class ControlChannelService(
    private val context: Context,
    private val host: String,
    private val controlPort: Int,
    private val onVolumeChanged: ((volumePct: Int) -> Unit)? = null,
) {
    companion object {
        const val TAG = "ControlChannelService"
        const val RECONNECT_DELAY_MS = 5000L
    }

    private val running = AtomicBoolean(false)
    private val audioManager = context.getSystemService(Context.AUDIO_SERVICE) as AudioManager
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private var socket: Socket? = null
    private var writer: OutputStreamWriter? = null

    fun start() {
        if (!running.compareAndSet(false, true)) return
        Log.i(TAG, "Starting control channel to $host:$controlPort")
        scope.launch {
            while (isActive && running.get()) {
                try {
                    connectAndListen()
                } catch (e: Exception) {
                    Log.w(TAG, "Control channel error: ${e.message}")
                }
                if (running.get()) {
                    delay(RECONNECT_DELAY_MS)
                }
            }
        }
    }

    fun stop() {
        running.set(false)
        scope.cancel()
        try {
            socket?.close()
        } catch (_: Exception) {}
        socket = null
        writer = null
        Log.i(TAG, "Control channel stopped")
    }

    fun reportVolumeChange(volumePct: Int) {
        scope.launch {
            try {
                val w = writer ?: return@launch
                val msg = JSONObject().apply {
                    put("type", "volume_changed")
                    put("volume_pct", volumePct)
                }
                synchronized(w) {
                    w.write(msg.toString() + "\n")
                    w.flush()
                }
            } catch (_: Exception) {}
        }
    }

    private suspend fun connectAndListen() {
        withContext(Dispatchers.IO) {
            socket = Socket(host, controlPort)
            writer = OutputStreamWriter(socket!!.getOutputStream())
            val reader = BufferedReader(InputStreamReader(socket!!.getInputStream()))

            Log.i(TAG, "Connected to control channel $host:$controlPort")

            // Report initial volume
            val maxVol = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
            val curVol = audioManager.getStreamVolume(AudioManager.STREAM_MUSIC)
            val initialPct = if (maxVol > 0) (curVol * 100 / maxVol) else 50
            val initMsg = JSONObject().apply {
                put("type", "volume_changed")
                put("volume_pct", initialPct)
            }
            synchronized(writer!!) {
                writer!!.write(initMsg.toString() + "\n")
                writer!!.flush()
            }

            var line: String?
            while (running.get() && isActive) {
                line = reader.readLine() ?: break
                if (line.isNullOrBlank()) continue
                try {
                    val msg = JSONObject(line.trim())
                    handleMessage(msg)
                } catch (e: Exception) {
                    Log.w(TAG, "Invalid control message: $line — ${e.message}")
                }
            }
        }
    }

    private fun handleMessage(msg: JSONObject) {
        val type = msg.optString("type", "")
        when (type) {
            "set_volume" -> {
                val pct = msg.optInt("volume_pct", -1)
                if (pct < 0 || pct > 100) return
                applyVolumePct(pct)
            }
            "volume" -> {
                val value = msg.optDouble("value", -1.0)
                if (value < 0.0 || value > 1.0) return
                val pct = (value * 100.0).toInt().coerceIn(0, 100)
                applyVolumePct(pct)
            }
            "ping" -> {
                // Keep-alive, no action needed
            }
            else -> {
                Log.d(TAG, "Unknown control message type: $type")
            }
        }
    }

    private fun applyVolumePct(pct: Int) {
        val maxVol = audioManager.getStreamMaxVolume(AudioManager.STREAM_MUSIC)
        val targetVol = (pct * maxVol / 100).coerceIn(0, maxVol)
        try {
            audioManager.setStreamVolume(AudioManager.STREAM_MUSIC, targetVol, 0)
            Log.i(TAG, "Volume set to $pct% ($targetVol/$maxVol)")
            onVolumeChanged?.invoke(pct)
        } catch (e: SecurityException) {
            Log.w(TAG, "Cannot set volume: ${e.message}")
        }
    }
}
