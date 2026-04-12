package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.net.wifi.WifiManager
import android.content.Context
import androidx.annotation.NonNull
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.EventChannel
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class MainActivity : FlutterActivity() {
    private val channelName = "lan_audio/audio_track"
    private val platformChannelName = "lan_audio/platform"
    private val playbackServiceChannelName = PlaybackChannels.METHOD_PLAYBACK_SERVICE
    private val playbackEventChannelName = PlaybackChannels.EVENT_PLAYBACK_EVENTS
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
                PlaybackForegroundService.stopPlayback(applicationContext)
                result.success(null)
            }

            "disconnect" -> {
                PlaybackForegroundService.stopPlayback(applicationContext)
                result.success(null)
            }

            "getSnapshot" -> {
                result.success(PlaybackEventBus.snapshotMap())
            }

            "setOptions" -> {
                val args = call.arguments as? Map<*, *> ?: emptyMap<String, Any?>()
                val startBufferMs = (args["startBufferMs"] as? Number)?.toInt() ?: 60
                val maxBufferMs = (args["maxBufferMs"] as? Number)?.toInt() ?: 300
                val pingIntervalMs = (args["pingIntervalMs"] as? Number)?.toInt() ?: 1000
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
