package com.example.lan_audio_android_mvp

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.PowerManager
import android.net.wifi.WifiManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

class PlaybackForegroundService : MediaSessionService() {
    private val logTag = "lan_audio_service"
    private val stateStore = PlaybackStateStore()
    private lateinit var sessionController: PlaybackSessionController
    private var mediaSession: MediaSession? = null
    private var player: ExoPlayer? = null
    private var foregroundStarted = false
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null

    private val storeListener: (PlaybackSnapshot) -> Unit = { snapshot ->
        updateNotification(snapshot)
    }

    override fun onCreate() {
        super.onCreate()
        Log.i(logTag, "onCreate")
        ensureNotificationChannel()
        sessionController = PlaybackSessionController(applicationContext, stateStore)
        player = ExoPlayer.Builder(this).build().apply {
            playWhenReady = false
            repeatMode = Player.REPEAT_MODE_OFF
        }
        mediaSession = MediaSession.Builder(this, player!!).build()
        stateStore.addListener(storeListener)
        stateStore.set(PlaybackSnapshot(serviceState = "idle", recentLog = "service_created"))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(logTag, "onStartCommand action=${intent?.action}")
        when (intent?.action) {
            PlaybackActions.ACTION_START -> {
                val host = intent.getStringExtra(PlaybackActions.EXTRA_HOST).orEmpty()
                val wsPort = intent.getIntExtra(PlaybackActions.EXTRA_WS_PORT, 39991)
                val udpPort = intent.getIntExtra(PlaybackActions.EXTRA_UDP_PORT, 39992)
                val serverName =
                    intent.getStringExtra(PlaybackActions.EXTRA_SERVER_NAME) ?: "manual:$host"
                if (host.isBlank()) {
                    Log.w(logTag, "startPlayback rejected: empty host")
                    stateStore.update {
                        it.copy(
                            serviceState = "error",
                            connectionState = "error",
                            playbackState = "stopped",
                            recentLog = "start_missing_host",
                            error = mapOf("code" to "missing_host", "message" to "host is required"),
                        )
                    }
                } else {
                    acquirePlaybackLocks()
                    Log.i(logTag, "startPlayback target=$serverName host=$host ws=$wsPort udp=$udpPort")
                    sessionController.startPlayback(
                        PlaybackTarget(
                            host = host,
                            wsPort = wsPort,
                            udpPort = udpPort,
                            serverName = serverName,
                        ),
                    )
                }
            }

            PlaybackActions.ACTION_STOP -> {
                Log.i(logTag, "stopPlayback from notification/service command")
                sessionController.stopPlayback("notification_stop")
                releasePlaybackLocks()
                stopForeground(STOP_FOREGROUND_REMOVE)
                stopSelf()
            }

            PlaybackActions.ACTION_RECONNECT -> {
                acquirePlaybackLocks()
                Log.i(logTag, "manual reconnect requested")
                sessionController.reconnect("notification_reconnect")
            }

            PlaybackActions.ACTION_SET_OPTIONS -> {
                val startBufferMs = intent.getIntExtra(PlaybackActions.EXTRA_START_BUFFER_MS, 60)
                val maxBufferMs = intent.getIntExtra(PlaybackActions.EXTRA_MAX_BUFFER_MS, 300)
                val pingIntervalMs = intent.getIntExtra(PlaybackActions.EXTRA_PING_INTERVAL_MS, 1000)
                sessionController.setOptions(
                    PlaybackOptions(
                        startBufferMs = startBufferMs,
                        maxBufferMs = maxBufferMs,
                        pingIntervalMs = pingIntervalMs,
                    ),
                )
            }
        }
        return START_STICKY
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        Log.i(logTag, "onDestroy")
        stateStore.removeListener(storeListener)
        sessionController.destroy()
        releasePlaybackLocks()
        mediaSession?.release()
        mediaSession = null
        player?.release()
        player = null
        super.onDestroy()
    }

    private fun updateNotification(snapshot: PlaybackSnapshot) {
        val notification = buildNotification(snapshot)
        if (!foregroundStarted) {
            startForeground(NOTIFICATION_ID, notification)
            foregroundStarted = true
            return
        }
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(NOTIFICATION_ID, notification)
    }

    private fun buildNotification(snapshot: PlaybackSnapshot): Notification {
        val targetLabel = when {
            snapshot.targetName != null && snapshot.targetHost != null ->
                "${snapshot.targetName} (${snapshot.targetHost})"

            snapshot.targetHost != null -> snapshot.targetHost
            else -> "LAN Audio"
        }
        val text = "${snapshot.connectionState}/${snapshot.playbackState}"

        val launchIntent = packageManager.getLaunchIntentForPackage(packageName)
        val launchPending = PendingIntent.getActivity(
            this,
            1,
            launchIntent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val stopPending = PendingIntent.getService(
            this,
            2,
            Intent(this, PlaybackForegroundService::class.java).setAction(PlaybackActions.ACTION_STOP),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val reconnectPending = PendingIntent.getService(
            this,
            3,
            Intent(this, PlaybackForegroundService::class.java).setAction(PlaybackActions.ACTION_RECONNECT),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        return NotificationCompat.Builder(this, NOTIFICATION_CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle(targetLabel)
            .setContentText(text)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(launchPending)
            .addAction(android.R.drawable.ic_menu_revert, "Reconnect", reconnectPending)
            .addAction(android.R.drawable.ic_media_pause, "Stop", stopPending)
            .build()
    }

    private fun ensureNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) {
            return
        }
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val channel = NotificationChannel(
            NOTIFICATION_CHANNEL_ID,
            "LAN Audio Playback",
            NotificationManager.IMPORTANCE_LOW,
        )
        channel.description = "LAN Audio background playback"
        manager.createNotificationChannel(channel)
    }

    private fun acquirePlaybackLocks() {
        if (wakeLock?.isHeld != true) {
            val powerManager = applicationContext.getSystemService(Context.POWER_SERVICE) as? PowerManager
            if (powerManager != null) {
                val lock = powerManager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, "lan_audio:playback")
                lock.setReferenceCounted(false)
                lock.acquire()
                wakeLock = lock
                Log.i(logTag, "PARTIAL_WAKE_LOCK acquired")
            }
        }

        if (wifiLock?.isHeld != true) {
            val wifiManager = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            if (wifiManager != null) {
                val lock = wifiManager.createWifiLock(
                    WifiManager.WIFI_MODE_FULL_HIGH_PERF,
                    "lan_audio:playback_wifi",
                )
                lock.setReferenceCounted(false)
                lock.acquire()
                wifiLock = lock
                Log.i(logTag, "WifiLock acquired")
            }
        }
    }

    private fun releasePlaybackLocks() {
        try {
            if (wakeLock?.isHeld == true) {
                wakeLock?.release()
            }
        } catch (_: Throwable) {
        } finally {
            wakeLock = null
        }

        try {
            if (wifiLock?.isHeld == true) {
                wifiLock?.release()
            }
        } catch (_: Throwable) {
        } finally {
            wifiLock = null
        }
        Log.i(logTag, "playback locks released")
    }

    companion object {
        private const val NOTIFICATION_CHANNEL_ID = "lan_audio_playback"
        private const val NOTIFICATION_ID = 2591

        fun startPlayback(context: Context, target: PlaybackTarget) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_START)
                .putExtra(PlaybackActions.EXTRA_HOST, target.host)
                .putExtra(PlaybackActions.EXTRA_WS_PORT, target.wsPort)
                .putExtra(PlaybackActions.EXTRA_UDP_PORT, target.udpPort)
                .putExtra(PlaybackActions.EXTRA_SERVER_NAME, target.serverName)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun stopPlayback(context: Context) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_STOP)
            context.startService(intent)
        }

        fun reconnect(context: Context) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_RECONNECT)
            context.startService(intent)
        }

        fun setOptions(context: Context, options: PlaybackOptions) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_SET_OPTIONS)
                .putExtra(PlaybackActions.EXTRA_START_BUFFER_MS, options.startBufferMs)
                .putExtra(PlaybackActions.EXTRA_MAX_BUFFER_MS, options.maxBufferMs)
                .putExtra(PlaybackActions.EXTRA_PING_INTERVAL_MS, options.pingIntervalMs)
            context.startService(intent)
        }
    }
}
