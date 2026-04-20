package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger

data class AudioTrackStats(
    val nativeQueuedFrames: Int,
    val nativeQueuedAudioFrames: Int,
    val audioTrackWriteFrames: Long,
    val audioTrackShortWriteCount: Long,
    val reportedLatencyMs: Int? = null,
    val lastPcmPeak: Int = 0,
    val lastPcmRms: Double = 0.0,
    val lastPlayState: Int = AudioTrack.PLAYSTATE_STOPPED,
)

class AudioTrackController {
    private val logTag = "lan_audio_track"
    private var audioTrack: AudioTrack? = null
    private var frameBytesPerPacket: Int = 1920
    private var audioEncoding: Int = AudioFormat.ENCODING_PCM_16BIT
    private var writeQueue: ArrayBlockingQueue<QueuedChunk>? = null
    private val queuedAudioFrames = AtomicInteger(0)
    private val queueFrameSoftCap = AtomicInteger(24)

    @Volatile
    private var writerRunning: Boolean = false

    @Volatile
    private var writeFrames: Long = 0

    @Volatile
    private var shortWriteCount: Long = 0

    @Volatile
    private var lastPcmPeak: Int = 0

    @Volatile
    private var lastPcmRms: Double = 0.0

    @Volatile
    private var lastWriterSummaryAtMs: Long = 0

    private var writerThread: Thread? = null
    @Volatile
    private var writerStoppedSignal: CountDownLatch? = null

    fun init(
        sampleRate: Int,
        channels: Int,
        frameSamplesPerChannel: Int,
        preferLowLatency: Boolean,
        encoding: Int = AudioFormat.ENCODING_PCM_16BIT,
    ) {
        Log.i(
            logTag,
            "audio writer init sampleRate=$sampleRate channels=$channels frameSamplesPerChannel=$frameSamplesPerChannel"
        )
        stopWriter()
        audioTrack?.release()
        writeFrames = 0
        shortWriteCount = 0
        queuedAudioFrames.set(0)

        val channelConfig = if (channels == 1) {
            AudioFormat.CHANNEL_OUT_MONO
        } else {
            AudioFormat.CHANNEL_OUT_STEREO
        }

        val channelCount = channels.coerceAtLeast(1)
        val minBuffer = AudioTrack.getMinBufferSize(
            sampleRate,
            channelConfig,
            encoding,
        )
        require(minBuffer > 0) { "AudioTrack.getMinBufferSize failed: $minBuffer" }

        val bytesPerSample = if (encoding == AudioFormat.ENCODING_PCM_FLOAT) 4 else 2
        val frameBytes = frameSamplesPerChannel * channelCount * bytesPerSample
        frameBytesPerPacket = frameBytes
        audioEncoding = encoding
        val desiredBuffer = maxOf(minBuffer * 2, frameBytes * 2)

        val builder = AudioTrack.Builder()
            .setAudioAttributes(
                AudioAttributes.Builder()
                    .setUsage(AudioAttributes.USAGE_MEDIA)
                    .setContentType(AudioAttributes.CONTENT_TYPE_MUSIC)
                    .build(),
            )
            .setAudioFormat(
                AudioFormat.Builder()
                    .setEncoding(encoding)
                    .setSampleRate(sampleRate)
                    .setChannelMask(channelConfig)
                    .build(),
            )
            .setBufferSizeInBytes(desiredBuffer)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setSessionId(AudioManager.AUDIO_SESSION_ID_GENERATE)

        if (preferLowLatency) {
            builder.setPerformanceMode(AudioTrack.PERFORMANCE_MODE_LOW_LATENCY)
        }

        val track = builder.build()
        if (track.state != AudioTrack.STATE_INITIALIZED) {
            track.release()
            throw IllegalStateException("AudioTrack init failed")
        }

        audioTrack = track
        track.setVolume(AudioTrack.getMaxVolume())
        startWriter()
    }

    fun start() {
        Log.i(logTag, "audio writer start")
        audioTrack?.play() ?: throw IllegalStateException("AudioTrack is not initialized")
    }

    fun setQueueSoftCapFrames(maxQueuedFrames: Int) {
        val clamped = maxQueuedFrames.coerceIn(4, 96)
        queueFrameSoftCap.set(clamped)
    }

    fun writePcm16(data: ByteArray, frames: Int) {
        val track = audioTrack ?: throw IllegalStateException("AudioTrack is not initialized")
        val queue = writeQueue ?: throw IllegalStateException("AudioTrack writer is not initialized")
        if (track.playState != AudioTrack.PLAYSTATE_PLAYING) {
            track.play()
        }
        val safeFrames = frames.coerceAtLeast(1)
        val copy = data.copyOf()
        if (queuedAudioFrames.get() >= queueFrameSoftCap.get()) {
            val dropped = queue.poll()
            if (dropped != null) {
                queuedAudioFrames.addAndGet(-dropped.frames)
            }
        }
        val chunk = QueuedChunk(copy, safeFrames)
        if (!queue.offer(chunk)) {
            val dropped = queue.poll()
            if (dropped != null) {
                queuedAudioFrames.addAndGet(-dropped.frames)
            }
            if (!queue.offer(chunk)) {
                return
            }
        }
        queuedAudioFrames.addAndGet(safeFrames)
    }

    fun stop() {
        Log.i(logTag, "audio writer stop")
        writeQueue?.clear()
        audioTrack?.pause()
        audioTrack?.flush()
    }

    fun release() {
        Log.i(logTag, "audio writer release")
        stopWriter()
        audioTrack?.release()
        audioTrack = null
    }

    fun stats(): AudioTrackStats {
        return AudioTrackStats(
            nativeQueuedFrames = writeQueue?.size ?: 0,
            nativeQueuedAudioFrames = queuedAudioFrames.get(),
            audioTrackWriteFrames = writeFrames,
            audioTrackShortWriteCount = shortWriteCount,
            reportedLatencyMs = readReportedLatencyMs(),
            lastPcmPeak = lastPcmPeak,
            lastPcmRms = lastPcmRms,
            lastPlayState = audioTrack?.playState ?: AudioTrack.PLAYSTATE_STOPPED,
        )
    }

    private fun startWriter() {
        val queue = ArrayBlockingQueue<QueuedChunk>(96)
        writeQueue = queue
        writerRunning = true
        val stopped = CountDownLatch(1)
        writerStoppedSignal = stopped
        Log.i(logTag, "audio writer thread started")
        writerThread = Thread({
            val threadName = Thread.currentThread().name
            val threadId = Process.myTid()
            try {
                Process.setThreadPriority(Process.THREAD_PRIORITY_URGENT_AUDIO)
                val effectivePriority = Process.getThreadPriority(threadId)
                Log.i(
                    logTag,
                    "audio writer thread priority set name=$threadName tid=$threadId requested=THREAD_PRIORITY_URGENT_AUDIO(${Process.THREAD_PRIORITY_URGENT_AUDIO}) effective=$effectivePriority",
                )
            } catch (t: Throwable) {
                Log.w(
                    logTag,
                    "audio writer thread priority set failed name=$threadName tid=$threadId requested=THREAD_PRIORITY_URGENT_AUDIO(${Process.THREAD_PRIORITY_URGENT_AUDIO}) error=${t.message}",
                )
            }
            try {
                while (writerRunning || queue.isNotEmpty()) {
                    val chunk = try {
                        queue.poll(50, TimeUnit.MILLISECONDS)
                    } catch (_: InterruptedException) {
                        break
                    } ?: continue
                    queuedAudioFrames.addAndGet(-chunk.frames)
                    val track = audioTrack ?: continue
                    writeFully(track, chunk)
                }
            } finally {
                stopped.countDown()
            }
        }, "lan-audio-service-track-writer").also { it.start() }
    }

    private fun stopWriter() {
        Log.i(logTag, "audio writer thread stopping")
        writerRunning = false
        writeQueue?.clear()
        queuedAudioFrames.set(0)
        writerThread?.interrupt()
        writerStoppedSignal?.await(100, TimeUnit.MILLISECONDS)
        writerThread = null
        writeQueue = null
        writerStoppedSignal = null
        Log.i(logTag, "audio writer thread stopped")
    }

    private fun writeFully(track: AudioTrack, chunk: QueuedChunk) {
        val data = chunk.payload
        updatePcmLevel(data)
        var offset = 0
        var shortWrite = false
        var zeroWriteRetries = 0
        while (offset < data.size) {
            val wrote = track.write(data, offset, data.size - offset, AudioTrack.WRITE_BLOCKING)
            if (wrote < 0) {
                shortWriteCount += 1
                Log.w(logTag, "AudioTrack.write returned error=$wrote; drop current frame")
                return
            }
            if (wrote == 0) {
                shortWrite = true
                zeroWriteRetries += 1
                if (!writerRunning || track.playState == AudioTrack.PLAYSTATE_STOPPED) {
                    Log.w(logTag, "AudioTrack.write returned 0 while stopping; drop current frame")
                    return
                }
                if (zeroWriteRetries >= 3) {
                    shortWriteCount += 1
                    Log.w(logTag, "AudioTrack.write returned 0 repeatedly; drop current frame")
                    return
                }
                try {
                    Thread.sleep(2)
                } catch (_: InterruptedException) {
                    Thread.currentThread().interrupt()
                    Log.i(logTag, "AudioTrack.write retry interrupted; drop current frame")
                    return
                }
                continue
            }
            if (wrote < data.size - offset) {
                shortWrite = true
            }
            offset += wrote
        }

        if (shortWrite) {
            shortWriteCount += 1
        }
        writeFrames += chunk.frames.toLong()
        maybeLogWriterSummary(track)
    }

    private fun updatePcmLevel(data: ByteArray) {
        if (data.size < 2) {
            lastPcmPeak = 0
            lastPcmRms = 0.0
            return
        }

        var peak = 0
        var sumSquares = 0.0
        var samples = 0
        var index = 0
        while (index + 1 < data.size) {
            val lo = data[index].toInt() and 0xFF
            val hi = data[index + 1].toInt()
            val sample = (hi shl 8) or lo
            val abs = kotlin.math.abs(sample)
            if (abs > peak) {
                peak = abs
            }
            val normalized = sample / 32768.0
            sumSquares += normalized * normalized
            samples += 1
            index += 2
        }
        lastPcmPeak = peak
        lastPcmRms = if (samples == 0) 0.0 else kotlin.math.sqrt(sumSquares / samples)
    }

    private fun maybeLogWriterSummary(track: AudioTrack) {
        val now = SystemClock.elapsedRealtime()
        val elapsed = now - lastWriterSummaryAtMs
        if (lastWriterSummaryAtMs != 0L && elapsed < 1000L) {
            return
        }
        lastWriterSummaryAtMs = now
        Log.i(
            logTag,
            "audio_writer_summary playState=${track.playState} queue=${writeQueue?.size ?: 0} queuedFrames=${queuedAudioFrames.get()} writeFrames=$writeFrames shortWrites=$shortWriteCount latencyMs=${readReportedLatencyMs()} pcmPeak=$lastPcmPeak pcmRms=$lastPcmRms encoding=$audioEncoding",
        )
    }

    private fun readReportedLatencyMs(): Int? {
        val track = audioTrack ?: return null
        return try {
            val method = AudioTrack::class.java.getMethod("getLatency")
            val value = method.invoke(track) as? Number
            value?.toInt()?.takeIf { it >= 0 }
        } catch (_: Throwable) {
            null
        }
    }

    private data class QueuedChunk(
        val payload: ByteArray,
        val frames: Int,
    )
}
