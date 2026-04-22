package com.example.lan_audio_android_mvp

import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import android.content.pm.ApplicationInfo
import android.util.Log

class DebugPlaybackReceiver : BroadcastReceiver() {
    override fun onReceive(context: Context, intent: Intent) {
        if ((context.applicationInfo.flags and ApplicationInfo.FLAG_DEBUGGABLE) == 0) {
            Log.w(logTag, "debug playback receiver ignored in non-debug build")
            return
        }

        when (intent.action) {
            ACTION_START_PLAYBACK -> {
                val host = intent.stringExtraOrFallback(
                    PlaybackActions.EXTRA_HOST,
                    EXTRA_DEBUG_HOST,
                ).orEmpty()
                if (host.isBlank()) {
                    Log.w(logTag, "debug start ignored: empty host")
                    return
                }
                val wsPort = intent.intExtraOrFallback(
                    PlaybackActions.EXTRA_WS_PORT,
                    EXTRA_DEBUG_WS_PORT,
                    39991,
                )
                val udpPort = intent.intExtraOrFallback(
                    PlaybackActions.EXTRA_UDP_PORT,
                    EXTRA_DEBUG_UDP_PORT,
                    39992,
                )
                val serverName = intent.stringExtraOrFallback(
                    PlaybackActions.EXTRA_SERVER_NAME,
                    EXTRA_DEBUG_SERVER_NAME,
                )
                    ?: "adb-debug:$host"
                val transportMode =
                    intent.stringExtraOrFallback(
                        PlaybackActions.EXTRA_TRANSPORT_MODE,
                        EXTRA_DEBUG_TRANSPORT_MODE,
                    ) ?: "wifi"
                Log.i(
                    logTag,
                    "debug start playback host=$host ws=$wsPort udp=$udpPort transport=$transportMode",
                )
                PlaybackForegroundService.startPlayback(
                    context.applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
                        transportMode = transportMode,
                    ),
                )
            }
            ACTION_STOP_PLAYBACK -> {
                Log.i(logTag, "debug stop playback")
                PlaybackForegroundService.stopPlayback(context.applicationContext)
            }
            ACTION_SET_AUDIO_MODE -> {
                val mode = intent.getStringExtra(PlaybackActions.EXTRA_AUDIO_MODE) ?: "balanced"
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "adb_debug"
                Log.i(logTag, "debug set audio mode=$mode reason=$reason")
                Log.i(
                    logTag,
                    "debug set extras keys=${intent.extras?.keySet()?.joinToString(",") ?: "none"} host=${intent.stringExtraOrFallback(PlaybackActions.EXTRA_HOST, EXTRA_DEBUG_HOST)} hostRaw=${intent.extras?.get(EXTRA_DEBUG_HOST)} transport=${intent.stringExtraOrFallback(PlaybackActions.EXTRA_TRANSPORT_MODE, EXTRA_DEBUG_TRANSPORT_MODE)} transportRaw=${intent.extras?.get(EXTRA_DEBUG_TRANSPORT_MODE)}",
                )
                PlaybackForegroundService.setAudioMode(context.applicationContext, mode, reason)
                val host = intent.stringExtraOrFallback(
                    PlaybackActions.EXTRA_HOST,
                    EXTRA_DEBUG_HOST,
                ).orEmpty()
                if (host.isNotBlank()) {
                    val wsPort = intent.intExtraOrFallback(
                        PlaybackActions.EXTRA_WS_PORT,
                        EXTRA_DEBUG_WS_PORT,
                        39991,
                    )
                    val udpPort = intent.intExtraOrFallback(
                        PlaybackActions.EXTRA_UDP_PORT,
                        EXTRA_DEBUG_UDP_PORT,
                        39992,
                    )
                    val serverName = intent.stringExtraOrFallback(
                        PlaybackActions.EXTRA_SERVER_NAME,
                        EXTRA_DEBUG_SERVER_NAME,
                    )
                        ?: "adb-debug:$host"
                    val transportMode =
                        intent.stringExtraOrFallback(
                            PlaybackActions.EXTRA_TRANSPORT_MODE,
                            EXTRA_DEBUG_TRANSPORT_MODE,
                        ) ?: "wifi"
                    Log.i(
                        logTag,
                        "debug set mode + start host=$host ws=$wsPort udp=$udpPort transport=$transportMode",
                    )
                    PlaybackForegroundService.startPlayback(
                        context.applicationContext,
                        PlaybackTarget(
                            host = host,
                            wsPort = wsPort,
                            udpPort = udpPort,
                            serverName = serverName,
                            transportMode = transportMode,
                        ),
                    )
                }
            }
            ACTION_DUMP_METRICS -> {
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "adb_debug"
                Log.i(logTag, "debug dump metrics reason=$reason")
                PlaybackForegroundService.dumpMetrics(context.applicationContext, reason)
            }
            ACTION_OPUS_SELF_TEST -> {
                runCatching {
                    val decoder = OpusNativeDecoder(48_000, 2)
                    try {
                        decoder.selfTestDecodePeak()
                    } finally {
                        decoder.release()
                    }
                }.onSuccess { peak ->
                    Log.i(logTag, "debug opus native self-test peak=$peak nonSilent=${peak > 0}")
                }.onFailure { t ->
                    Log.e(logTag, "debug opus native self-test failed", t)
                }
            }
        }
    }

    companion object {
        private const val logTag = "lan_audio_debug"
        const val ACTION_START_PLAYBACK = "lan_audio.debug.START_PLAYBACK"
        const val ACTION_STOP_PLAYBACK = "lan_audio.debug.STOP_PLAYBACK"
        const val ACTION_SET_AUDIO_MODE = "lan_audio.debug.SET_AUDIO_MODE"
        const val ACTION_DUMP_METRICS = "lan_audio.debug.DUMP_METRICS"
        const val ACTION_OPUS_SELF_TEST = "lan_audio.debug.OPUS_SELF_TEST"
        private const val EXTRA_DEBUG_HOST = "debug_host"
        private const val EXTRA_DEBUG_WS_PORT = "debug_ws_port"
        private const val EXTRA_DEBUG_UDP_PORT = "debug_udp_port"
        private const val EXTRA_DEBUG_SERVER_NAME = "debug_server_name"
        private const val EXTRA_DEBUG_TRANSPORT_MODE = "debug_transport_mode"
    }

    private fun Intent.stringExtraOrFallback(primary: String, secondary: String): String? {
        val direct = getStringExtra(primary)
        if (!direct.isNullOrBlank()) {
            return direct
        }
        val fallback = getStringExtra(secondary)
        if (!fallback.isNullOrBlank()) {
            return fallback
        }
        return extras?.get(primary)?.toString() ?: extras?.get(secondary)?.toString()
    }

    private fun Intent.intExtraOrFallback(primary: String, secondary: String, defaultValue: Int): Int {
        if (hasExtra(primary)) {
            return getIntExtra(primary, defaultValue)
        }
        if (hasExtra(secondary)) {
            return getIntExtra(secondary, defaultValue)
        }
        return defaultValue
    }
}
