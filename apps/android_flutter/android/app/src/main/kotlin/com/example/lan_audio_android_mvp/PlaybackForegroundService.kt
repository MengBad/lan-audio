package com.example.lan_audio_android_mvp

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.AlarmManager
import android.content.Context
import android.content.Intent
import android.support.v4.media.MediaMetadataCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import android.os.Build
import android.os.PowerManager
import android.os.SystemClock
import android.net.wifi.WifiManager
import android.util.Log
import androidx.core.app.NotificationCompat
import androidx.media.app.NotificationCompat.MediaStyle
import androidx.media3.common.Player
import androidx.media3.exoplayer.ExoPlayer
import androidx.media3.session.MediaSession
import androidx.media3.session.MediaSessionService

class PlaybackForegroundService : MediaSessionService() {
    private val logTag = "lan_audio_service"
    private val stateStore = PlaybackStateStore()
    private lateinit var sessionController: PlaybackSessionController
    private var mediaSession: MediaSession? = null
    private var mediaSessionCompat: MediaSessionCompat? = null
    private var player: ExoPlayer? = null
    private var foregroundStarted = false
    private var wakeLock: PowerManager.WakeLock? = null
    private var wifiLock: WifiManager.WifiLock? = null
    private var explicitStopRequested = false
    private var lastNotificationAtMs = 0L
    private var lastNotificationKey = ""
    private var lifecycleState = ServiceLifecycleState.IDLE

    private val storeListener: (PlaybackSnapshot) -> Unit = { snapshot ->
        syncLifecycleState(snapshot)
        updateMediaSession(snapshot)
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
        mediaSessionCompat = MediaSessionCompat(this, "LANAudioPlayback").apply {
            isActive = true
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = handlePlayPauseAction()
                override fun onPause() = handlePlayPauseAction()
                override fun onStop() = stopFromMediaAction("media_session_stop")
            })
        }
        stateStore.addListener(storeListener)
        stateStore.set(PlaybackSnapshot(serviceState = "idle", recentLog = "service_created"))
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        Log.i(logTag, "onStartCommand action=${intent?.action}")
        when (intent?.action) {
            PlaybackActions.ACTION_START -> {
                syncLifecycleState(stateStore.current())
                if (lifecycleState != ServiceLifecycleState.IDLE) {
                    Log.w(logTag, "ACTION_START ignored in lifecycleState=$lifecycleState")
                    stateStore.update {
                        it.copy(recentLog = "start_ignored_state:${lifecycleState.name.lowercase()}")
                    }
                    return START_STICKY
                }
                val host = intent.getStringExtra(PlaybackActions.EXTRA_HOST).orEmpty()
                val wsPort = intent.getIntExtra(PlaybackActions.EXTRA_WS_PORT, 39991)
                val udpPort = intent.getIntExtra(PlaybackActions.EXTRA_UDP_PORT, 39992)
                val serverName =
                    intent.getStringExtra(PlaybackActions.EXTRA_SERVER_NAME) ?: "manual:$host"
                val transportMode =
                    intent.getStringExtra(PlaybackActions.EXTRA_TRANSPORT_MODE) ?: "wifi"
                if (host.isBlank()) {
                    Log.w(logTag, "startPlayback rejected: empty host")
                    transitionLifecycle(ServiceLifecycleState.ERROR, "start_missing_host")
                    transitionLifecycle(ServiceLifecycleState.IDLE, "start_missing_host:error_idle")
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
                    if (shouldIgnoreStartPlayback(host)) {
                        Log.w(logTag, "startPlayback ignored: active session host=$host")
                        stateStore.update {
                            it.copy(recentLog = "start_ignored_active:$host")
                        }
                        return START_STICKY
                    }
                    explicitStopRequested = false
                    persistTarget(host, wsPort, udpPort, serverName, transportMode)
                    acquirePlaybackLocks()
                    transitionLifecycle(ServiceLifecycleState.CONNECTING, "action_start")
                    Log.i(logTag, "startPlayback target=$serverName host=$host ws=$wsPort udp=$udpPort transport=$transportMode")
                    sessionController.startPlayback(
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

            PlaybackActions.ACTION_STOP -> {
                Log.i(logTag, "stopPlayback from notification/service command")
                stopFromMediaAction("notification_stop")
            }

            PlaybackActions.ACTION_PLAY_PAUSE -> {
                handlePlayPauseAction()
            }

            PlaybackActions.ACTION_RECONNECT -> {
                acquirePlaybackLocks()
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "notification_reconnect"
                Log.i(logTag, "manual reconnect requested reason=$reason")
                if (sessionController.hasActiveTarget()) {
                    sessionController.reconnect(reason)
                } else {
                    restorePersistedPlayback(reason)
                }
            }

            PlaybackActions.ACTION_RESTORE_LAST -> {
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "app_open_restore"
                val snapshot = stateStore.current()
                if (sessionController.hasActiveTarget() && snapshot.connectionState != "connected") {
                    Log.i(logTag, "restore last reconnects active target reason=$reason state=${snapshot.connectionState}")
                    sessionController.reconnect(reason)
                } else if (sessionController.hasActiveTarget()) {
                    Log.i(logTag, "restore last ignored: active target already exists reason=$reason")
                } else {
                    Log.i(logTag, "restore last requested reason=$reason")
                    restorePersistedPlayback(reason)
                }
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

            PlaybackActions.ACTION_SET_AUDIO_MODE -> {
                val mode = intent.getStringExtra(PlaybackActions.EXTRA_AUDIO_MODE) ?: "balanced"
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "ui_request"
                sessionController.setAudioMode(mode, reason)
            }

            PlaybackActions.ACTION_DUMP_METRICS -> {
                val reason = intent.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "adb_request"
                sessionController.dumpMetrics(reason)
            }

            else -> {
                if (intent == null) {
                    Log.w(logTag, "sticky restart with null intent; trying persisted target")
                    restorePersistedPlayback("sticky_restart")
                }
            }
        }
        return START_STICKY
    }

    override fun onGetSession(controllerInfo: MediaSession.ControllerInfo): MediaSession? {
        return mediaSession
    }

    override fun onDestroy() {
        val snapshot = stateStore.current()
        Log.i(
            logTag,
            "onDestroy serviceState=${snapshot.serviceState} connectionState=${snapshot.connectionState} playbackState=${snapshot.playbackState}"
        )
        stateStore.removeListener(storeListener)
        if (!explicitStopRequested && shouldRestoreAfterDestroy(snapshot)) {
            schedulePlaybackRestore("service_destroyed")
        }
        sessionController.destroy()
        releasePlaybackLocks()
        mediaSessionCompat?.release()
        mediaSessionCompat = null
        mediaSession?.release()
        mediaSession = null
        player?.release()
        player = null
        super.onDestroy()
    }

    private fun updateNotification(snapshot: PlaybackSnapshot) {
        val now = SystemClock.elapsedRealtime()
        val notificationKey = listOf(
            snapshot.serviceState,
            snapshot.connectionState,
            snapshot.playbackState,
            snapshot.targetHost ?: "",
            snapshot.targetName ?: "",
            snapshot.currentAudioMode,
            snapshot.protocolPath,
            snapshot.effectiveCodec,
        ).joinToString("|")
        if (foregroundStarted &&
            notificationKey == lastNotificationKey &&
            now - lastNotificationAtMs < NOTIFICATION_UPDATE_MIN_INTERVAL_MS
        ) {
            return
        }

        val notification = buildNotification(snapshot)
        lastNotificationAtMs = now
        lastNotificationKey = notificationKey
        if (!foregroundStarted) {
            Log.i(
                logTag,
                "service entered foreground target=${snapshot.targetName ?: snapshot.targetHost ?: "LAN Audio"} state=${snapshot.connectionState}/${snapshot.playbackState}"
            )
            startForeground(NOTIFICATION_ID, notification)
            foregroundStarted = true
            return
        }
        val manager = getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        manager.notify(NOTIFICATION_ID, notification)
    }

    override fun onTaskRemoved(rootIntent: Intent?) {
        val snapshot = stateStore.current()
        Log.w(
            logTag,
            "onTaskRemoved foregroundStarted=$foregroundStarted serviceState=${snapshot.serviceState} connectionState=${snapshot.connectionState} playbackState=${snapshot.playbackState}"
        )
        stopFromMediaAction("task_removed")
        super.onTaskRemoved(rootIntent)
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
        val playPausePending = PendingIntent.getService(
            this,
            3,
            Intent(this, PlaybackForegroundService::class.java).setAction(PlaybackActions.ACTION_PLAY_PAUSE),
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )

        return NotificationCompat.Builder(this, NOTIFICATION_CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_media_play)
            .setContentTitle("LAN Audio")
            .setContentText(text)
            .setSubText(targetLabel)
            .setOngoing(true)
            .setOnlyAlertOnce(true)
            .setContentIntent(launchPending)
            .setStyle(MediaStyle().setMediaSession(mediaSessionCompat?.sessionToken))
            .addAction(android.R.drawable.ic_media_play, "Play/Pause", playPausePending)
            .addAction(android.R.drawable.ic_media_pause, "Stop", stopPending)
            .build()
    }

    private fun stopFromMediaAction(reason: String) {
        syncLifecycleState(stateStore.current())
        if (lifecycleState == ServiceLifecycleState.IDLE) {
            Log.i(logTag, "stop ignored in IDLE reason=$reason")
            stateStore.update { it.copy(recentLog = "stop_noop_idle:$reason") }
            return
        }
        transitionLifecycle(ServiceLifecycleState.STOPPING, reason)
        explicitStopRequested = true
        clearPersistedTarget()
        sessionController.stopPlayback(reason)
        releasePlaybackLocks()
        stopForeground(STOP_FOREGROUND_REMOVE)
        transitionLifecycle(ServiceLifecycleState.IDLE, "$reason:stopped")
        stopSelf()
    }

    private fun handlePlayPauseAction() {
        val snapshot = stateStore.current()
        if (snapshot.playbackState == "playing" || snapshot.connectionState == "connected") {
            stopFromMediaAction("notification_play_pause")
            return
        }
        Log.i(logTag, "play/pause ignored while inactive to avoid implicit start")
        stateStore.update {
            it.copy(recentLog = "play_pause_ignored_inactive")
        }
    }

    private fun syncLifecycleState(snapshot: PlaybackSnapshot) {
        if (lifecycleState == ServiceLifecycleState.STOPPING) {
            return
        }
        if (snapshot.connectionState == "error" || snapshot.serviceState == "error") {
            transitionLifecycle(ServiceLifecycleState.ERROR, "snapshot:${snapshot.recentLog}")
            transitionLifecycle(ServiceLifecycleState.IDLE, "snapshot_error_idle:${snapshot.recentLog}")
            return
        }
        val next = when {
            snapshot.connectionState == "connecting" || snapshot.connectionState == "reconnecting" ->
                ServiceLifecycleState.CONNECTING
            snapshot.connectionState == "connected" ||
                snapshot.playbackState == "playing" ||
                snapshot.playbackState == "buffering" ->
                ServiceLifecycleState.PLAYING
            snapshot.serviceState == "idle" && snapshot.connectionState != "connected" ->
                ServiceLifecycleState.IDLE
            else -> lifecycleState
        }
        transitionLifecycle(next, "snapshot")
    }

    private fun transitionLifecycle(next: ServiceLifecycleState, reason: String) {
        if (lifecycleState == next) {
            return
        }
        Log.i(logTag, "service_lifecycle ${lifecycleState.name}->${next.name} reason=$reason")
        lifecycleState = next
    }

    private fun shouldIgnoreStartPlayback(host: String): Boolean {
        if (!sessionController.hasActiveTarget()) {
            return false
        }
        val snapshot = stateStore.current()
        val activeSession =
            snapshot.serviceState == "running" ||
                snapshot.connectionState == "connecting" ||
                snapshot.connectionState == "connected" ||
                snapshot.connectionState == "reconnecting" ||
                snapshot.playbackState == "buffering" ||
                snapshot.playbackState == "playing"
        return activeSession && !host.isBlank()
    }

    private fun updateMediaSession(snapshot: PlaybackSnapshot) {
        val compat = mediaSessionCompat ?: return
        val mappedState = when {
            snapshot.connectionState == "error" || snapshot.serviceState == "error" -> PlaybackStateCompat.STATE_ERROR
            snapshot.connectionState == "connecting" || snapshot.connectionState == "reconnecting" -> PlaybackStateCompat.STATE_CONNECTING
            snapshot.playbackState == "playing" -> PlaybackStateCompat.STATE_PLAYING
            else -> PlaybackStateCompat.STATE_STOPPED
        }
        compat.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_STOP)
                .setState(mappedState, PlaybackStateCompat.PLAYBACK_POSITION_UNKNOWN, 1.0f)
                .build()
        )
        compat.setMetadata(
            MediaMetadataCompat.Builder()
                .putString(MediaMetadataCompat.METADATA_KEY_TITLE, "LAN Audio")
                .putString(
                    MediaMetadataCompat.METADATA_KEY_ARTIST,
                    snapshot.targetHost ?: snapshot.targetName ?: "unknown",
                )
                .build()
        )
        compat.isActive = true
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

    private fun persistTarget(
        host: String,
        wsPort: Int,
        udpPort: Int,
        serverName: String,
        transportMode: String,
    ) {
        getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .putString(KEY_HOST, host)
            .putInt(KEY_WS_PORT, wsPort)
            .putInt(KEY_UDP_PORT, udpPort)
            .putString(KEY_SERVER_NAME, serverName)
            .putString(KEY_TRANSPORT_MODE, transportMode)
            .putBoolean(KEY_AUTO_RESTORE, true)
            .apply()
    }

    private fun clearPersistedTarget() {
        getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
            .edit()
            .clear()
            .apply()
    }

    private fun restorePersistedPlayback(reason: String): Boolean {
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        if (!prefs.getBoolean(KEY_AUTO_RESTORE, false)) {
            Log.w(logTag, "restore skipped: auto restore disabled reason=$reason")
            return false
        }
        val host = prefs.getString(KEY_HOST, null).orEmpty()
        if (host.isBlank()) {
            Log.w(logTag, "restore skipped: missing persisted host reason=$reason")
            return false
        }
        explicitStopRequested = false
        acquirePlaybackLocks()
        val target = PlaybackTarget(
            host = host,
            wsPort = prefs.getInt(KEY_WS_PORT, 39991),
            udpPort = prefs.getInt(KEY_UDP_PORT, 39992),
            serverName = prefs.getString(KEY_SERVER_NAME, null) ?: "manual:$host",
            transportMode = prefs.getString(KEY_TRANSPORT_MODE, "wifi") ?: "wifi",
        )
        Log.i(logTag, "restore persisted playback reason=$reason target=${target.serverName} host=${target.host}")
        sessionController.startPlayback(target)
        return true
    }

    private fun shouldRestoreAfterDestroy(snapshot: PlaybackSnapshot): Boolean {
        if (explicitStopRequested) {
            return false
        }
        if (!hasPersistedTarget()) {
            return false
        }
        return snapshot.serviceState == "running" ||
            snapshot.connectionState == "connected" ||
            snapshot.connectionState == "connecting" ||
            snapshot.connectionState == "reconnecting" ||
            snapshot.playbackState == "playing" ||
            snapshot.playbackState == "buffering"
    }

    private fun hasPersistedTarget(): Boolean {
        val prefs = getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        return prefs.getBoolean(KEY_AUTO_RESTORE, false) && !prefs.getString(KEY_HOST, null).isNullOrBlank()
    }

    private fun schedulePlaybackRestore(reason: String) {
        try {
            val intent = Intent(this, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_RECONNECT)
                .putExtra(PlaybackActions.EXTRA_REASON, "auto_restore:$reason")
            val flags = PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
            val pendingIntent = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                PendingIntent.getForegroundService(this, RESTORE_REQUEST_CODE, intent, flags)
            } else {
                PendingIntent.getService(this, RESTORE_REQUEST_CODE, intent, flags)
            }
            val alarmManager = getSystemService(Context.ALARM_SERVICE) as AlarmManager
            val triggerAt = SystemClock.elapsedRealtime() + RESTORE_DELAY_MS
            alarmManager.set(AlarmManager.ELAPSED_REALTIME_WAKEUP, triggerAt, pendingIntent)
            Log.w(logTag, "scheduled playback restore reason=$reason delayMs=$RESTORE_DELAY_MS")
        } catch (t: Throwable) {
            Log.e(logTag, "schedule playback restore failed reason=$reason error=${t.message}")
        }
    }

    companion object {
        private const val NOTIFICATION_CHANNEL_ID = "lan_audio_playback"
        private const val NOTIFICATION_ID = 2591
        private const val NOTIFICATION_UPDATE_MIN_INTERVAL_MS = 1000L
        private const val RESTORE_REQUEST_CODE = 2592
        private const val RESTORE_DELAY_MS = 2500L
        private const val PREFS_NAME = "lan_audio_playback_restore"
        private const val KEY_HOST = "host"
        private const val KEY_WS_PORT = "ws_port"
        private const val KEY_UDP_PORT = "udp_port"
        private const val KEY_SERVER_NAME = "server_name"
        private const val KEY_TRANSPORT_MODE = "transport_mode"
        private const val KEY_AUTO_RESTORE = "auto_restore"

        fun startPlayback(context: Context, target: PlaybackTarget) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_START)
                .putExtra(PlaybackActions.EXTRA_HOST, target.host)
                .putExtra(PlaybackActions.EXTRA_WS_PORT, target.wsPort)
                .putExtra(PlaybackActions.EXTRA_UDP_PORT, target.udpPort)
                .putExtra(PlaybackActions.EXTRA_SERVER_NAME, target.serverName)
                .putExtra(PlaybackActions.EXTRA_TRANSPORT_MODE, target.transportMode)
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

        fun restoreLastPlayback(context: Context, reason: String = "app_open_restore") {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_RESTORE_LAST)
                .putExtra(PlaybackActions.EXTRA_REASON, reason)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                context.startForegroundService(intent)
            } else {
                context.startService(intent)
            }
        }

        fun setOptions(context: Context, options: PlaybackOptions) {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_SET_OPTIONS)
                .putExtra(PlaybackActions.EXTRA_START_BUFFER_MS, options.startBufferMs)
                .putExtra(PlaybackActions.EXTRA_MAX_BUFFER_MS, options.maxBufferMs)
                .putExtra(PlaybackActions.EXTRA_PING_INTERVAL_MS, options.pingIntervalMs)
            context.startService(intent)
        }

        fun setAudioMode(context: Context, mode: String, reason: String = "ui_request") {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_SET_AUDIO_MODE)
                .putExtra(PlaybackActions.EXTRA_AUDIO_MODE, mode)
                .putExtra(PlaybackActions.EXTRA_REASON, reason)
            context.startService(intent)
        }

        fun dumpMetrics(context: Context, reason: String = "manual_request") {
            val intent = Intent(context, PlaybackForegroundService::class.java)
                .setAction(PlaybackActions.ACTION_DUMP_METRICS)
                .putExtra(PlaybackActions.EXTRA_REASON, reason)
            context.startService(intent)
        }
    }
}

private enum class ServiceLifecycleState {
    IDLE,
    CONNECTING,
    PLAYING,
    STOPPING,
    ERROR,
}
