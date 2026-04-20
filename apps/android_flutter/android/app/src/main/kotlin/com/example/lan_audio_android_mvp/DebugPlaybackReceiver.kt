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
                val host = intent.getStringExtra(PlaybackActions.EXTRA_HOST).orEmpty()
                if (host.isBlank()) {
                    Log.w(logTag, "debug start ignored: empty host")
                    return
                }
                val wsPort = intent.getIntExtra(PlaybackActions.EXTRA_WS_PORT, 39991)
                val udpPort = intent.getIntExtra(PlaybackActions.EXTRA_UDP_PORT, 39992)
                val serverName = intent.getStringExtra(PlaybackActions.EXTRA_SERVER_NAME)
                    ?: "adb-debug:$host"
                Log.i(logTag, "debug start playback host=$host ws=$wsPort udp=$udpPort")
                PlaybackForegroundService.startPlayback(
                    context.applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
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
                PlaybackForegroundService.setAudioMode(context.applicationContext, mode, reason)
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
    }
}
