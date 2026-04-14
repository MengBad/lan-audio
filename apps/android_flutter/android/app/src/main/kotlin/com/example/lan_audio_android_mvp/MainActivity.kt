package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.net.wifi.WifiManager
import android.content.Context
import android.content.Intent
import androidx.annotation.NonNull
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import android.util.Log
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MainActivity : FlutterActivity() {
    private companion object {
        const val PREFS_NAME = "lan_audio_prefs"
        const val KEY_FIRST_USE_HINT_CONSUMED = "first_use_hint_consumed"
    }

    private val channelName = "lan_audio/audio_track"
    private val platformChannelName = "lan_audio/platform"
    private val playbackServiceChannelName = PlaybackChannels.METHOD_PLAYBACK_SERVICE
    private val playbackEventChannelName = PlaybackChannels.EVENT_PLAYBACK_EVENTS
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

    override fun onCreate(savedInstanceState: android.os.Bundle?) {
        super.onCreate(savedInstanceState)
        Log.i(logTag, "onCreate action=${intent?.action} extras=${intent?.extras?.keySet()?.joinToString(",") ?: ""}")
        logLifecycle("onCreate")
        handleDebugCommand(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        setIntent(intent)
        Log.i(logTag, "onNewIntent action=${intent.action} extras=${intent.extras?.keySet()?.joinToString(",") ?: ""}")
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
        super.onDestroy()
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
    }

    private fun handleDebugCommand(intent: Intent?) {
        val incoming = intent ?: return
        val command = incoming.getStringExtra("debug_command")?.trim().orEmpty()
        if (command.isBlank()) {
            return
        }
        when (command) {
            "start_playback" -> {
                val host = incoming.getStringExtra(PlaybackActions.EXTRA_HOST).orEmpty()
                if (host.isBlank()) {
                    Log.w(logTag, "debug start_playback ignored: empty host")
                    return
                }
                val wsPort = incoming.getIntExtra(PlaybackActions.EXTRA_WS_PORT, 39991)
                val udpPort = incoming.getIntExtra(PlaybackActions.EXTRA_UDP_PORT, 39992)
                val serverName =
                    incoming.getStringExtra(PlaybackActions.EXTRA_SERVER_NAME) ?: "manual:$host"
                Log.i(logTag, "debug start_playback host=$host ws=$wsPort udp=$udpPort")
                PlaybackForegroundService.startPlayback(
                    applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
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
    }

    private fun preferences() =
        applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)

    private fun logLifecycle(name: String) {
        val snapshot = PlaybackEventBus.snapshotMap()
        Log.i(
            logTag,
            "$name serviceState=${snapshot["serviceState"]} connectionState=${snapshot["connectionState"]} playbackState=${snapshot["playbackState"]}"
        )
    }

    private fun handlePlaybackServiceCall(call: MethodCall, result: MethodChannel.Result) {
        when (call.method) {
            "startPlayback" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val host = args["host"] as? String ?: ""
                val wsPort = (args["wsPort"] as? Number)?.toInt() ?: 39991
                val udpPort = (args["udpPort"] as? Number)?.toInt() ?: 39992
                val serverName = args["serverName"] as? String ?: "manual:$host"
                if (host.isBlank()) {
                    result.error("invalid_args", "host is required", null)
                    return
                }
                Log.i(
                    logTag,
                    "MethodChannel startPlayback received host=$host ws=$wsPort udp=$udpPort server=$serverName"
                )
                PlaybackForegroundService.startPlayback(
                    applicationContext,
                    PlaybackTarget(
                        host = host,
                        wsPort = wsPort,
                        udpPort = udpPort,
                        serverName = serverName,
                    ),
                )
                result.success(null)
            }

            "stopPlayback" -> {
                Log.i(logTag, "MethodChannel stopPlayback received")
                PlaybackForegroundService.stopPlayback(applicationContext)
                result.success(null)
            }

            "disconnect" -> {
                Log.i(logTag, "MethodChannel disconnect received")
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
                if (mode.isBlank()) {
                    result.error("invalid_args", "mode is required", null)
                    return
                }
                Log.i(logTag, "MethodChannel setAudioMode received mode=$mode reason=$reason")
                PlaybackForegroundService.setAudioMode(applicationContext, mode, reason)
                result.success(null)
            }

            else -> result.notImplemented()
        }
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
