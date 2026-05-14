package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.net.nsd.NsdManager
import android.net.nsd.NsdServiceInfo
import android.net.wifi.WifiManager
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.net.Uri
import android.os.Build
import androidx.annotation.NonNull
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import android.util.Log
import java.net.Inet4Address
import java.net.InetAddress
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

class MainActivity : FlutterActivity() {
    private companion object {
        const val PREFS_NAME = "lan_audio_prefs"
        const val KEY_FIRST_USE_HINT_CONSUMED = "first_use_hint_consumed"
        const val KEY_CONNECT_HISTORY_JSON = "connect_history_json"
        const val NSD_SERVICE_TYPE = "_lan-audio._tcp"
        const val REQUEST_CODE_MIC_PERMISSION = 2701
        val ACTIVE_PLAYBACK_STATES = setOf(
            "handshaking",
            "negotiated",
            "streaming",
            "recovering",
            "reconfiguring",
        )
    }

    private val channelName = "lan_audio/audio_track"
    private val platformChannelName = "lan_audio/platform"
    private val playbackServiceChannelName = PlaybackChannels.METHOD_PLAYBACK_SERVICE
    private val playbackEventChannelName = PlaybackChannels.EVENT_PLAYBACK_EVENTS
    private val micChannelName = "lan_audio/mic"
    private val jitterMetricsChannelName = "lan_audio/jitter_metrics"
    private val logTag = "lan_audio_activity"
    private var audioTrack: AudioTrack? = null
    private var sampleRate: Int = 48000
    private var channels: Int = 2
    private var frameBytesPerPacket: Int = 1920
    private var writeQueue: ArrayBlockingQueue<ByteArray>? = null
    @Volatile private var writerRunning: Boolean = false
    @Volatile private var writeFrames: Long = 0
    @Volatile private var shortWriteCount: Long = 0
    private var writerThread: Thread? = null
    @Volatile private var writerStoppedSignal: CountDownLatch? = null
    private var multicastLock: WifiManager.MulticastLock? = null
    private val uiScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    @Volatile private var pendingPowerGuideRequest: Boolean = false
    private var nsdManager: NsdManager? = null
    private var nsdDiscoveryListener: NsdManager.DiscoveryListener? = null
    private val nsdServices = ConcurrentHashMap<String, Map<String, Any>>()
    private var pendingMicPermissionResult: MethodChannel.Result? = null
    @Volatile private var micCaptureService: MicCaptureService? = null
    @Volatile private var micPeakDb: Float = -96f
    @Volatile private var micRmsDb: Float = -96f
    @Volatile private var controlChannelService: ControlChannelService? = null
    @Volatile private var jitterEventSink: EventChannel.EventSink? = null
    @Volatile private var jitterPollerActive = false

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(logTag, "onCreate action=${intent?.action} extras=${intent?.extras?.keySet()?.joinToString(",") ?: ""}")
        consumePowerGuideIntent(intent)
        logLifecycle("onCreate")
        val handledDebug = handleDebugCommand(intent)
        if (!handledDebug) {
            if (shouldRestorePlaybackOnAppOpen()) {
                PlaybackForegroundService.restoreLastPlayback(applicationContext, "app_open_restore")
            } else {
                Log.i(logTag, "skip app_open_restore because playback is already active")
            }
        } else {
            Log.i(logTag, "skip app_open_restore because debug_command handled")
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        Log.i(logTag, "onNewIntent action=${intent.action} extras=${intent.extras?.keySet()?.joinToString(",") ?: ""}")
        consumePowerGuideIntent(intent)
        logLifecycle("onNewIntent")
        handleDebugCommand(intent)
    }

    override fun onResume() {
        super.onResume()
        logLifecycle("onResume")
    }

    override fun onPause() {
        super.onPause()
        logLifecycle("onPause")
    }

    override fun onStop() {
        super.onStop()
        logLifecycle("onStop")
    }

    override fun onDestroy() {
        logLifecycle("onDestroy no service stop issued from activity lifecycle")
        stopNsdDiscovery()
        stopMicCaptureInternal()
        uiScope.coroutineContext.cancel()
        super.onDestroy()
    }

    override fun onRequestPermissionsResult(
        requestCode: Int,
        permissions: Array<out String>,
        grantResults: IntArray,
    ) {
        super.onRequestPermissionsResult(requestCode, permissions, grantResults)
        if (requestCode == REQUEST_CODE_MIC_PERMISSION) {
            val granted = grantResults.isNotEmpty() &&
                grantResults[0] == PackageManager.PERMISSION_GRANTED
            pendingMicPermissionResult?.success(granted)
            pendingMicPermissionResult = null
        }
    }

    override fun configureFlutterEngine(@NonNull flutterEngine: FlutterEngine) {
        super.configureFlutterEngine(flutterEngine)
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, channelName)
            .setMethodCallHandler { call, result ->
                try {
                    handleCall(call, result)
                } catch (e: Exception) {
                    result.error("audio_track_error", e.message, null)
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, platformChannelName)
            .setMethodCallHandler { call, result ->
                try {
                    when (call.method) {
                        "acquireMulticastLock" -> {
                            acquireMulticastLock()
                            result.success(null)
                        }
                        "releaseMulticastLock" -> {
                            releaseMulticastLock()
                            result.success(null)
                        }
                        "getFirstUseHintConsumed" -> {
                            result.success(preferences().getBoolean(KEY_FIRST_USE_HINT_CONSUMED, false))
                        }
                        "setFirstUseHintConsumed" -> {
                            val consumed =
                                (call.arguments as? Map<*, *>)?.get("consumed") as? Boolean ?: true
                            preferences().edit().putBoolean(KEY_FIRST_USE_HINT_CONSUMED, consumed).apply()
                            result.success(null)
                        }
                        "getDeviceManufacturer" -> {
                            result.success(Build.MANUFACTURER ?: "")
                        }
                        "getConnectHistory" -> {
                            result.success(preferences().getString(KEY_CONNECT_HISTORY_JSON, "") ?: "")
                        }
                        "setConnectHistory" -> {
                            val raw = (call.arguments as? Map<*, *>)?.get("json") as? String ?: "[]"
                            preferences().edit().putString(KEY_CONNECT_HISTORY_JSON, raw).apply()
                            result.success(null)
                        }
                        "consumePowerGuideRequest" -> {
                            val requested = pendingPowerGuideRequest
                            pendingPowerGuideRequest = false
                            result.success(requested)
                        }
                        "startNsdDiscovery" -> {
                            startNsdDiscovery()
                            result.success(null)
                        }
                        "stopNsdDiscovery" -> {
                            stopNsdDiscovery()
                            result.success(null)
                        }
                        "getNsdDiscoveredServices" -> {
                            result.success(nsdServices.values.toList())
                        }
                        "checkForAppUpdate" -> {
                            val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                            val delayMs = (args["delayMs"] as? Number)?.toLong() ?: 0L
                            uiScope.launch {
                                if (delayMs > 0) {
                                    delay(delayMs)
                                }
                                val currentVersion = currentVersionName()
                                val update = withContext(Dispatchers.IO) {
                                    UpdateChecker.checkForUpdate(currentVersion)
                                }
                                if (update == null) {
                                    result.success(null)
                                } else {
                                    result.success(
                                        mapOf(
                                            "latestVersion" to update.latestVersion,
                                            "releaseUrl" to update.releaseUrl,
                                        )
                                    )
                                }
                            }
                        }
                        "openExternalUrl" -> {
                            val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                            val url = (args["url"] as? String).orEmpty().trim()
                            if (url.isBlank()) {
                                result.success(null)
                                return@setMethodCallHandler
                            }
                            try {
                                startActivity(Intent(Intent.ACTION_VIEW, Uri.parse(url)))
                            } catch (_: Throwable) {
                                // silent ignore
                            }
                            result.success(null)
                        }
                        "getMicRationaleString" -> {
                            val resId = resources.getIdentifier(
                                "mic_permission_rationale", "string", packageName)
                            val text = if (resId != 0) getString(resId)
                                else "Microphone access is needed to stream your voice to the PC audio system. No audio is recorded or saved."
                            result.success(text)
                        }
                        "requestMicPermission" -> {
                            pendingMicPermissionResult = result
                            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                                requestPermissions(
                                    arrayOf(android.Manifest.permission.RECORD_AUDIO),
                                    REQUEST_CODE_MIC_PERMISSION)
                            } else {
                                result.success(true)
                                pendingMicPermissionResult = null
                            }
                        }
                        else -> result.notImplemented()
                    }
                } catch (e: Exception) {
                    result.error("platform_error", e.message, null)
                }
            }
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, playbackServiceChannelName)
            .setMethodCallHandler { call, result ->
                try {
                    handlePlaybackServiceCall(call, result)
                } catch (e: Exception) {
                    result.error("playback_service_error", e.message, null)
                }
            }
        EventChannel(flutterEngine.dartExecutor.binaryMessenger, playbackEventChannelName)
            .setStreamHandler(object : EventChannel.StreamHandler {
                override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
                    PlaybackEventBus.attachSink(events)
                }

                override fun onCancel(arguments: Any?) {
                    PlaybackEventBus.detachSink()
                }
            })
        MethodChannel(flutterEngine.dartExecutor.binaryMessenger, micChannelName)
            .setMethodCallHandler { call, result ->
                try {
                    handleMicCall(call, result)
                } catch (e: Exception) {
                    result.error("mic_error", e.message, null)
                }
            }
        EventChannel(flutterEngine.dartExecutor.binaryMessenger, jitterMetricsChannelName)
            .setStreamHandler(object : EventChannel.StreamHandler {
                override fun onListen(arguments: Any?, events: EventChannel.EventSink) {
                    jitterEventSink = events
                    startJitterPoller()
                }

                override fun onCancel(arguments: Any?) {
                    jitterEventSink = null
                    stopJitterPoller()
                }
            })
    }

    private fun handleDebugCommand(intent: Intent?): Boolean {
        val incoming = intent ?: return false
        val command = incoming.getStringExtra("debug_command")?.trim().orEmpty()
        if (command.isBlank()) {
            return false
        }
        when (command) {
            "start_playback" -> {
                val host = incoming.getStringExtra(PlaybackActions.EXTRA_HOST).orEmpty()
                if (host.isBlank()) {
                    Log.w(logTag, "debug start_playback ignored: empty host")
                    return true
                }
                val wsPort = incoming.getIntExtra(PlaybackActions.EXTRA_WS_PORT, 39991)
                val udpPort = incoming.getIntExtra(PlaybackActions.EXTRA_UDP_PORT, 39992)
                val serverName =
                    incoming.getStringExtra(PlaybackActions.EXTRA_SERVER_NAME) ?: "manual:$host"
                val transportMode =
                    incoming.getStringExtra(PlaybackActions.EXTRA_TRANSPORT_MODE) ?: "wifi"
                Log.i(logTag, "debug start_playback host=$host ws=$wsPort udp=$udpPort")
                PlaybackForegroundService.startPlayback(
                    applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
                        transportMode = transportMode,
                    ),
                )
            }

            "stop_playback" -> {
                Log.i(logTag, "debug stop_playback")
                PlaybackForegroundService.stopPlayback(applicationContext)
            }

            "set_audio_mode" -> {
                val mode = incoming.getStringExtra(PlaybackActions.EXTRA_AUDIO_MODE) ?: "balanced"
                val reason = incoming.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "adb_debug"
                Log.i(logTag, "debug set_audio_mode mode=$mode reason=$reason")
                PlaybackForegroundService.setAudioMode(applicationContext, mode, reason)
            }

            "dump_metrics" -> {
                val reason = incoming.getStringExtra(PlaybackActions.EXTRA_REASON) ?: "adb_debug"
                Log.i(logTag, "debug dump_metrics reason=$reason")
                PlaybackForegroundService.dumpMetrics(applicationContext, reason)
            }
        }
        return true
    }

    private fun preferences() =
        applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private fun startNsdDiscovery() {
        stopNsdDiscovery()
        nsdServices.clear()
        val manager = applicationContext.getSystemService(Context.NSD_SERVICE) as NsdManager
        nsdManager = manager
        val listener = object : NsdManager.DiscoveryListener {
            override fun onDiscoveryStarted(serviceType: String) {
                Log.i(logTag, "NSD discovery started type=$serviceType")
            }

            override fun onServiceFound(serviceInfo: NsdServiceInfo) {
                if (!serviceInfo.serviceType.contains(NSD_SERVICE_TYPE)) {
                    return
                }
                resolveNsdService(serviceInfo)
            }

            override fun onServiceLost(serviceInfo: NsdServiceInfo) {
                val key = serviceInfo.serviceName
                nsdServices.remove(key)
                Log.i(logTag, "NSD service lost ${serviceInfo.serviceName}")
            }

            override fun onDiscoveryStopped(serviceType: String) {
                Log.i(logTag, "NSD discovery stopped type=$serviceType")
            }

            override fun onStartDiscoveryFailed(serviceType: String, errorCode: Int) {
                Log.w(logTag, "NSD start failed type=$serviceType error=$errorCode")
                stopNsdDiscovery()
            }

            override fun onStopDiscoveryFailed(serviceType: String, errorCode: Int) {
                Log.w(logTag, "NSD stop failed type=$serviceType error=$errorCode")
            }
        }
        nsdDiscoveryListener = listener
        manager.discoverServices(NSD_SERVICE_TYPE, NsdManager.PROTOCOL_DNS_SD, listener)
    }

    private fun stopNsdDiscovery() {
        val manager = nsdManager
        val listener = nsdDiscoveryListener
        nsdDiscoveryListener = null
        if (manager != null && listener != null) {
            try {
                manager.stopServiceDiscovery(listener)
            } catch (_: Throwable) {
                // Discovery may already be stopped by the platform.
            }
        }
    }

    private fun resolveNsdService(serviceInfo: NsdServiceInfo) {
        val manager = nsdManager ?: return
        manager.resolveService(serviceInfo, object : NsdManager.ResolveListener {
            override fun onResolveFailed(info: NsdServiceInfo, errorCode: Int) {
                Log.w(logTag, "NSD resolve failed name=${info.serviceName} error=$errorCode")
            }

            override fun onServiceResolved(resolved: NsdServiceInfo) {
                val host = resolved.host ?: return
                val ipv4 = ipv4Address(host) ?: return
                val port = resolved.port.takeIf { it > 0 } ?: 39991
                val version = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                    resolved.attributes["version"]?.toString(Charsets.UTF_8) ?: ""
                } else {
                    ""
                }
                val mode = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.LOLLIPOP) {
                    resolved.attributes["mode"]?.toString(Charsets.UTF_8) ?: ""
                } else {
                    ""
                }
                val key = "${ipv4.hostAddress}:$port"
                nsdServices[key] = mapOf(
                    "serverId" to "mdns-$key",
                    "serverName" to resolved.serviceName,
                    "host" to (ipv4.hostAddress ?: ""),
                    "wsPort" to port,
                    "udpPort" to 39992,
                    "version" to version,
                    "mode" to mode,
                )
                Log.i(logTag, "NSD service resolved ${resolved.serviceName} ${ipv4.hostAddress}:$port")
            }
        })
    }

    private fun ipv4Address(address: InetAddress): Inet4Address? =
        when (address) {
            is Inet4Address -> address
            else -> null
        }

    private fun consumePowerGuideIntent(intent: Intent?) {
        if (intent?.getBooleanExtra(PlaybackActions.EXTRA_OPEN_POWER_GUIDE, false) == true) {
            pendingPowerGuideRequest = true
        }
    }

    private fun currentVersionName(): String {
        return try {
            val packageInfo = packageManager.getPackageInfo(packageName, 0)
            packageInfo.versionName ?: "0.0.0"
        } catch (_: PackageManager.NameNotFoundException) {
            "0.0.0"
        }
    }

    private fun logLifecycle(name: String) {
        val snapshot = PlaybackEventBus.snapshotMap()
        Log.i(
            logTag,
            "$name state=${snapshot["state"]} transport=${snapshot["transport"]} data_plane=${snapshot["data_plane"]}"
        )
    }

    private fun shouldRestorePlaybackOnAppOpen(): Boolean {
        val snapshot = PlaybackEventBus.snapshotMap()
        val state = snapshot["state"] as? String ?: return true
        return state !in ACTIVE_PLAYBACK_STATES
    }

    private fun handlePlaybackServiceCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "startPlayback" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val host = args["host"] as? String ?: ""
                val wsPort = (args["wsPort"] as? Number)?.toInt() ?: 39991
                val udpPort = (args["udpPort"] as? Number)?.toInt() ?: 39992
                val serverName = args["serverName"] as? String ?: "manual:$host"
                val transportMode = args["transportMode"] as? String ?: "wifi"
                if (host.isBlank()) {
                    result.error("invalid_args", "host is required", null)
                    return
                }
                Log.i(
                    logTag,
                    "MethodChannel startPlayback received host=$host ws=$wsPort udp=$udpPort server=$serverName transport=$transportMode"
                )
                PlaybackForegroundService.startPlayback(
                    applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
                        transportMode = transportMode,
                    ),
                )
                startControlChannel(host)
                result.success(null)
            }

            "stopPlayback" -> {
                Log.i(logTag, "MethodChannel stopPlayback received")
                stopControlChannel()
                PlaybackForegroundService.stopPlayback(applicationContext)
                result.success(null)
            }

            "disconnect" -> {
                Log.i(logTag, "MethodChannel disconnect received")
                stopControlChannel()
                PlaybackForegroundService.stopPlayback(applicationContext)
                result.success(null)
            }

            "getSnapshot" -> {
                Log.i(logTag, "MethodChannel getSnapshot received")
                result.success(PlaybackEventBus.snapshotMap())
            }

            "setOptions" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val startBufferMs = (args["startBufferMs"] as? Number)?.toInt() ?: 60
                val maxBufferMs = (args["maxBufferMs"] as? Number)?.toInt() ?: 300
                val pingIntervalMs = (args["pingIntervalMs"] as? Number)?.toInt() ?: 1000
                Log.i(
                    logTag,
                    "MethodChannel setOptions received startBufferMs=$startBufferMs maxBufferMs=$maxBufferMs pingIntervalMs=$pingIntervalMs"
                )
                PlaybackForegroundService.setOptions(
                    applicationContext,
                    PlaybackOptions(
                        startBufferMs = startBufferMs,
                        maxBufferMs = maxBufferMs,
                        pingIntervalMs = pingIntervalMs,
                    ),
                )
                result.success(null)
            }

            "setAudioMode" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val mode = (args["mode"] as? String)?.trim().orEmpty()
                val reason = (args["reason"] as? String)?.trim().orEmpty().ifBlank { "ui_request" }
                val preferredCodec = (args["preferredCodec"] as? String)
                    ?.trim()
                    ?.takeIf { it.isNotEmpty() }
                if (mode.isBlank()) {
                    result.error("invalid_args", "mode is required", null)
                    return
                }
                Log.i(
                    logTag,
                    "MethodChannel setAudioMode received mode=$mode reason=$reason codec=${preferredCodec ?: "default"}",
                )
                PlaybackForegroundService.setAudioMode(
                    applicationContext,
                    mode,
                    reason,
                    preferredCodec,
                )
                result.success(null)
            }

            "setEqSettings" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val settings = PlaybackEqSettings(
                    enabled = args["enabled"] == true,
                    lowDb = (args["lowDb"] as? Number)?.toInt() ?: 0,
                    midDb = (args["midDb"] as? Number)?.toInt() ?: 0,
                    highDb = (args["highDb"] as? Number)?.toInt() ?: 0,
                ).clamped()
                Log.i(
                    logTag,
                    "MethodChannel setEqSettings received enabled=${settings.enabled} low=${settings.lowDb} mid=${settings.midDb} high=${settings.highDb}",
                )
                PlaybackForegroundService.setEqSettings(applicationContext, settings)
                result.success(null)
            }

            "setLoudnessNormalization" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val enabled = args["enabled"] == true
                Log.i(logTag, "MethodChannel setLoudnessNormalization received enabled=$enabled")
                PlaybackForegroundService.setLoudnessNormalization(applicationContext, enabled)
                result.success(null)
            }

            else -> result.notImplemented()
        }
    }

    private fun handleMicCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "startMicCapture" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val host = args["host"] as? String ?: ""
                val reversePort = (args["reversePort"] as? Number)?.toInt() ?: 7878
                if (host.isBlank()) {
                    result.error("invalid_args", "host is required", null)
                    return
                }
                Log.i(logTag, "startMicCapture host=$host reversePort=$reversePort")
                stopMicCaptureInternal()
                val service = MicCaptureService(
                    host = host,
                    reversePort = reversePort,
                    onLevel = { peakDb, rmsDb ->
                        micPeakDb = peakDb
                        micRmsDb = rmsDb
                    },
                    onError = { error ->
                        Log.e(logTag, "MicCaptureService error: $error")
                        stopMicCaptureInternal()
                    }
                )
                micCaptureService = service
                service.start()
                PlaybackForegroundService.notifyMicStarted(applicationContext, host)
                result.success(null)
            }

            "stopMicCapture" -> {
                Log.i(logTag, "stopMicCapture")
                stopMicCaptureInternal()
                result.success(null)
            }

            "getMicLevel" -> {
                result.success(
                    mapOf(
                        "peakDb" to micPeakDb,
                        "rmsDb" to micRmsDb,
                    )
                )
            }

            else -> result.notImplemented()
        }
    }

    private fun startControlChannel(host: String) {
        stopControlChannel()
        val messenger = flutterEngine?.dartExecutor?.binaryMessenger ?: run {
            Log.w(logTag, "Cannot start control channel: no Flutter engine")
            return
        }
        val platformChannel = MethodChannel(messenger, platformChannelName)
        val service = ControlChannelService(
            context = applicationContext,
            host = host,
            controlPort = 7879,
            onVolumeChanged = { volumePct ->
                Log.i(logTag, "Volume changed from control channel: $volumePct%")
                try {
                    platformChannel.invokeMethod("volumeChanged", mapOf("volume_pct" to volumePct))
                } catch (_: Exception) {}
            },
        )
        controlChannelService = service
        service.start()
        Log.i(logTag, "Control channel started for $host:7879")
    }

    private fun stopControlChannel() {
        controlChannelService?.stop()
        controlChannelService = null
    }

    private fun startJitterPoller() {
        if (jitterPollerActive) return
        jitterPollerActive = true
        uiScope.launch {
            while (isActive && jitterPollerActive) {
                val sink = jitterEventSink ?: break
                try {
                    val snapshot = PlaybackEventBus.snapshotMap()
                    val metrics = snapshot["metrics"] as? Map<*, *>
                    val jitterHistory = (metrics?.get("jitter_history_us") as? List<*>)?.mapNotNull {
                        (it as? Number)?.toInt()
                    } ?: emptyList()
                    val jitterP50Us = (metrics?.get("jitter_p50_us") as? Number)?.toInt() ?: 0
                    val jitterP95Us = (metrics?.get("jitter_p95_ms") as? Number)?.let { it.toInt() * 1000 } ?: 0
                    val underrun = (metrics?.get("jitterUnderrun") as? Number)?.toInt() ?: 0

                    val payload: Map<String, Any?> = mapOf(
                        "jitterHistoryUs" to jitterHistory,
                        "jitterP50Us" to jitterP50Us,
                        "jitterP95Us" to jitterP95Us,
                        "underrunCount" to underrun,
                    )
                    sink.success(payload)
                } catch (_: Exception) {}
                delay(100L)
            }
        }
    }

    private fun stopJitterPoller() {
        jitterPollerActive = false
    }

    private fun stopMicCaptureInternal() {
        micCaptureService?.stop()
        micCaptureService = null
        PlaybackForegroundService.notifyMicStopped(applicationContext)
    }

    private fun handleCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "init" -> {
                val sr = call.argument<Int>("sampleRate") ?: 48000
                val ch = call.argument<Int>("channels") ?: 2
                val frameSamplesPerChannel = call.argument<Int>("frameSamplesPerChannel") ?: 480
                initAudioTrack(sr, ch, frameSamplesPerChannel)
                result.success(null)
            }

            "start" -> {
                audioTrack?.play() ?: throw IllegalStateException("AudioTrack is not initialized")
                result.success(null)
            }

            "writePcm16" -> {
                val data = call.arguments as? ByteArray
                    ?: throw IllegalArgumentException("writePcm16 expects ByteArray")
                enqueuePcm(data)
                result.success(null)
            }

            "stats" -> {
                result.success(
                    mapOf(
                        "nativeQueuedFrames" to (writeQueue?.size ?: 0),
                        "audioTrackWriteFrames" to writeFrames,
                        "audioTrackShortWriteCount" to shortWriteCount,
                    )
                )
            }

            "stop" -> {
                writeQueue?.clear()
                audioTrack?.pause()
                audioTrack?.flush()
                result.success(null)
            }

            "release" -> {
                stopWriter()
                audioTrack?.release()
                audioTrack = null
                result.success(null)
            }

            else -> result.notImplemented()
        }
    }

    private fun initAudioTrack(sr: Int, ch: Int, frameSamplesPerChannel: Int) {
        stopWriter()
        audioTrack?.release()

        sampleRate = sr
        channels = ch
        writeFrames = 0
        shortWriteCount = 0

        val channelConfig = if (ch == 1) {
            AudioFormat.CHANNEL_OUT_MONO
        } else {
            AudioFormat.CHANNEL_OUT_STEREO
        }

        val minBuffer = AudioTrack.getMinBufferSize(
            sampleRate,
            channelConfig,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        if (minBuffer <= 0) {
            throw IllegalStateException("AudioTrack.getMinBufferSize failed: $minBuffer")
        }

        val frameBytes = frameSamplesPerChannel * channels * 2
        frameBytesPerPacket = frameBytes
        // Keep a larger stream buffer to reduce jitter-induced starvation spikes.
        val desiredBuffer = maxOf(minBuffer, frameBytes * 12)

        val track = AudioTrack(
            AudioAttributes.Builder()
                .setUsage(AudioAttributes.USAGE_MEDIA)
                .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                .build(),
            AudioFormat.Builder()
                .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
                .setSampleRate(sampleRate)
                .setChannelMask(channelConfig)
                .build(),
            desiredBuffer,
            AudioTrack.MODE_STREAM,
            AudioManager.AUDIO_SESSION_ID_GENERATE,
        )

        if (track.state != AudioTrack.STATE_INITIALIZED) {
            track.release()
            throw IllegalStateException("AudioTrack init failed")
        }

        audioTrack = track
        startWriter()
    }

    private fun startWriter() {
        val queue = ArrayBlockingQueue<ByteArray>(240)
        writeQueue = queue
        writerRunning = true
        val stopped = CountDownLatch(1)
        writerStoppedSignal = stopped
        writerThread = Thread({
            try {
                while (writerRunning || queue.isNotEmpty()) {
                    val data = try {
                        queue.poll(50, TimeUnit.MILLISECONDS)
                    } catch (_: InterruptedException) {
                        break
                    } ?: continue
                    val track = audioTrack ?: continue
                    writeFully(track, data)
                }
            } finally {
                stopped.countDown()
            }
        }, "lan-audio-track-writer").also { it.start() }
    }

    private fun stopWriter() {
        writerRunning = false
        writeQueue?.clear()
        writerThread?.interrupt()
        writerStoppedSignal?.await(100, TimeUnit.MILLISECONDS)
        writerThread = null
        writeQueue = null
        writerStoppedSignal = null
    }

    private fun enqueuePcm(data: ByteArray) {
        audioTrack ?: throw IllegalStateException("AudioTrack is not initialized")
        val queue = writeQueue ?: throw IllegalStateException("AudioTrack writer is not initialized")
        val copy = data.copyOf()
        if (!queue.offer(copy)) {
            queue.poll()
            queue.offer(copy)
        }
    }

    private fun writeFully(track: AudioTrack, data: ByteArray) {
        var offset = 0
        var shortWrite = false
        while (offset < data.size) {
            val wrote = track.write(data, offset, data.size - offset, AudioTrack.WRITE_BLOCKING)
            if (wrote <= 0) {
                throw IllegalStateException("AudioTrack.write failed: $wrote")
            }
            if (wrote < data.size - offset) {
                shortWrite = true
            }
            offset += wrote
        }
        if (shortWrite) {
            shortWriteCount += 1
        }
        val perFrame = frameBytesPerPacket.coerceAtLeast(1)
        val framesInWrite = (data.size / perFrame).coerceAtLeast(1)
        writeFrames += framesInWrite.toLong()
    }

    private fun acquireMulticastLock() {
        if (multicastLock?.isHeld == true) {
            return
        }
        val wifiManager = applicationContext.getSystemService(Context.WIFI_SERVICE) as? WifiManager
            ?: throw IllegalStateException("WifiManager unavailable")
        val lock = wifiManager.createMulticastLock("lan_audio_discovery_lock")
        lock.setReferenceCounted(false)
        lock.acquire()
        multicastLock = lock
    }

    private fun releaseMulticastLock() {
        multicastLock?.let {
            if (it.isHeld) {
                it.release()
            }
        }
        multicastLock = null
    }
}
