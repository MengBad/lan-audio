package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.Process
import android.os.SystemClock
import android.util.Log
import java.util.Locale
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong

class AudioTrackController : PlaybackAudioSink {
    private val logTag = "lan_audio_track"
    private var audioTrack: AudioTrack? = null
    private var frameBytesPerPacket: Int = 1920
    private var frameDurationMs: Int = 20
    private var audioEncoding: Int = AudioFormat.ENCODING_PCM_16BIT
    private var silenceBuffer: ByteArray = ByteArray(0)
    private var transportHint: TransportHint = TransportHint.Wifi
    private var writeQueue: ArrayBlockingQueue<QueuedChunk>? = null
    private val queuedAudioFrames = AtomicInteger(0)
    private val queueFrameSoftCap = AtomicInteger(24)

    @Volatile
    private var writerRunning: Boolean = false
    @Volatile
    private var playRequested: Boolean = false
    @Volatile
    private var playbackStarted: Boolean = false
    @Volatile
    private var playRequestedAtMs: Long = 0
    @Volatile
    private var lastPrefillWaitLogAtMs: Long = 0
    @Volatile
    private var startupPendingLogged: Boolean = false

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

    @Volatile
    private var lastWriteCallAtMs: Long = 0

    @Volatile
    private var lastUnderrunSampleAtMs: Long = 0

    @Volatile
    private var lastUnderrunCount: Int? = null
    @Volatile
    private var packetLossFillCount: Long = 0
    @Volatile
    private var lastPacketLossFillCount: Long = 0
    @Volatile
    private var lastWifiSummaryAtMs: Long = 0
    @Volatile
    private var lastArrivalAtMs: Long = 0
    private val arrivalIntervalMs = ArrayDeque<Long>(100)
    private val enqueuedCount = AtomicLong(0)
    private val enqueueDroppedCount = AtomicLong(0)
    private val consumedCount = AtomicLong(0)
    private val writeCount = AtomicLong(0)
    private val pcmWriteCount = AtomicLong(0)
    private var lastLoggedEnqueuedCount: Long = 0
    private var lastLoggedEnqueueDroppedCount: Long = 0
    private var lastLoggedConsumedCount: Long = 0
    private var lastLoggedWriteCount: Long = 0
    private var lastLoggedPcmWriteCount: Long = 0
    private var writeDurationWindowNsSum: Long = 0
    private var writeDurationWindowSamples: Long = 0
    private var writeDurationWindowNsMax: Long = 0
    private val writeGapWindowMs = ArrayDeque<Long>(128)
    private val queueStatsLock = Any()
    private var queueSizeWindowMin = Int.MAX_VALUE
    private var queueSizeWindowMax = 0
    private var queueSizeWindowSum = 0L
    private var queueSizeWindowSamples = 0L

    private var writerThread: Thread? = null
    @Volatile
    private var writerStoppedSignal: CountDownLatch? = null

    override fun init(
        sampleRate: Int,
        channels: Int,
        frameSamplesPerChannel: Int,
        transportHint: TransportHint,
        encoding: Int,
    ) {
        Log.i(
            logTag,
            "audio writer init sampleRate=$sampleRate channels=$channels frameSamplesPerChannel=$frameSamplesPerChannel"
        )
        stopWriter("reinit")
        audioTrack?.release()
        writeFrames = 0
        shortWriteCount = 0
        queuedAudioFrames.set(0)
        lastWriteCallAtMs = 0
        lastUnderrunSampleAtMs = 0
        lastUnderrunCount = null
        packetLossFillCount = 0
        lastPacketLossFillCount = 0
        lastWifiSummaryAtMs = 0
        lastArrivalAtMs = 0
        synchronized(arrivalIntervalMs) {
            arrivalIntervalMs.clear()
        }
        synchronized(writeGapWindowMs) {
            writeGapWindowMs.clear()
        }
        playRequested = false
        playbackStarted = false
        enqueuedCount.set(0)
        enqueueDroppedCount.set(0)
        consumedCount.set(0)
        writeCount.set(0)
        pcmWriteCount.set(0)
        lastLoggedEnqueuedCount = 0
        lastLoggedEnqueueDroppedCount = 0
        lastLoggedConsumedCount = 0
        lastLoggedWriteCount = 0
        lastLoggedPcmWriteCount = 0
        writeDurationWindowNsSum = 0
        writeDurationWindowSamples = 0
        writeDurationWindowNsMax = 0
        synchronized(queueStatsLock) {
            queueSizeWindowMin = Int.MAX_VALUE
            queueSizeWindowMax = 0
            queueSizeWindowSum = 0L
            queueSizeWindowSamples = 0L
        }

        val channelConfig = if (channels == 1) {
            AudioFormat.CHANNEL_OUT_MONO
        } else {
            AudioFormat.CHANNEL_OUT_STEREO
        }

        val channelCount = channels.coerceAtLeast(1)
        val nativeSampleRate = queryNativeOutputSampleRate()
        val trackSampleRate =
            if (isSupportedPreferredSampleRate(nativeSampleRate)) nativeSampleRate else sampleRate
        val srcNeeded = sampleRate != nativeSampleRate
        val minBuffer = AudioTrack.getMinBufferSize(
            trackSampleRate,
            channelConfig,
            encoding,
        )
        require(minBuffer > 0) { "AudioTrack.getMinBufferSize failed: $minBuffer" }

        val bytesPerSample = if (encoding == AudioFormat.ENCODING_PCM_FLOAT) 4 else 2
        val frameBytes = frameSamplesPerChannel * channelCount * bytesPerSample
        frameBytesPerPacket = frameBytes
        frameDurationMs = ((frameSamplesPerChannel * 1000) / sampleRate.coerceAtLeast(1)).coerceAtLeast(1)
        silenceBuffer = ByteArray(frameBytes.coerceAtLeast(0))
        this.transportHint = transportHint
        audioEncoding = encoding
        val track = buildAudioTrack(
            sampleRate = trackSampleRate,
            channelConfig = channelConfig,
            encoding = encoding,
            minBufferSizeBytes = minBuffer,
        )
        if (track.state != AudioTrack.STATE_INITIALIZED) {
            track.release()
            throw IllegalStateException("AudioTrack init failed")
        }

        val actualMode =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) track.performanceMode else null
        val actualCapacityFrames =
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) track.bufferCapacityInFrames else null
        val actualBufBytes = actualCapacityFrames?.times(channelCount)?.times(bytesPerSample)
        val configuredBufferBytes = configuredBufferSizeBytes(
            track = track,
            channelCount = channelCount,
            bytesPerSample = bytesPerSample,
            fallbackBytes = minBuffer,
        )
        Log.i(
            logTag,
            "sample_rate_check requested=$sampleRate device_native=$nativeSampleRate",
        )
        Log.i(
            logTag,
            "init_report mode=${actualMode ?: "n/a"} minBuf=$minBuffer actualBuf=${actualBufBytes ?: configuredBufferBytes} requested_sr=$sampleRate native_sr=$nativeSampleRate src_needed=$srcNeeded",
        )
        Log.i(
            logTag,
            "audio writer init buffers minBufferSizeBytes=$minBuffer configuredBufferSizeBytes=$configuredBufferBytes bufferCapacityFrames=${actualCapacityFrames ?: "n/a"} performanceMode=${actualMode ?: "n/a"} transport=${transportHint.name.lowercase()} trackSampleRate=$trackSampleRate",
        )

        audioTrack = track
        track.setVolume(AudioTrack.getMaxVolume())
        startWriter()
    }

    override fun start() {
        Log.i(logTag, "audio writer start")
        try {
            Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
        } catch (_: Throwable) {
        }
        if (audioTrack == null) {
            throw IllegalStateException("AudioTrack is not initialized")
        }
        playRequested = true
        playRequestedAtMs = SystemClock.elapsedRealtime()
        lastPrefillWaitLogAtMs = 0L
        startupPendingLogged = false
    }

    override fun setQueueSoftCapFrames(maxQueuedFrames: Int) {
        val clamped = maxQueuedFrames.coerceIn(4, 96)
        queueFrameSoftCap.set(clamped)
    }

    override fun writePcm16(data: ByteArray, frames: Int) {
        audioTrack ?: throw IllegalStateException("AudioTrack is not initialized")
        val queue = writeQueue ?: throw IllegalStateException("AudioTrack writer is not initialized")
        val safeFrames = frames.coerceAtLeast(1)
        val copy = data.copyOf()
        val chunk = QueuedChunk(copy, safeFrames)
        try {
            queue.put(chunk)
            enqueuedCount.incrementAndGet()
            sampleQueueSize(queue.size)
            recordArrivalInterval()
        } catch (_: InterruptedException) {
            enqueueDroppedCount.incrementAndGet()
            Thread.currentThread().interrupt()
            return
        }
        queuedAudioFrames.addAndGet(safeFrames)
    }

    override fun stop() {
        Log.i(logTag, "audio writer stop")
        writeQueue?.clear()
        audioTrack?.pause()
        audioTrack?.flush()
        resetStartupState("stop")
    }

    override fun release() {
        Log.i(logTag, "audio writer release")
        stopWriter("release")
        audioTrack?.release()
        audioTrack = null
    }

    override fun stats(): PlaybackAudioSinkStats {
        return PlaybackAudioSinkStats(
            nativeQueuedFrames = writeQueue?.size ?: 0,
            nativeQueuedAudioFrames = queuedAudioFrames.get(),
            audioTrackWriteFrames = writeFrames,
            audioTrackShortWriteCount = shortWriteCount,
            writeGapP95Ms = writeGapP95Ms(),
            reportedLatencyMs = readReportedLatencyMs(),
            lastPcmPeak = lastPcmPeak,
            lastPcmRms = lastPcmRms,
            lastPlayState = audioTrack?.playState ?: AudioTrack.PLAYSTATE_STOPPED,
            silenceFillTotal = packetLossFillCount,
            underrunTotal = lastUnderrunCount ?: 0,
            ringBufferLevelFrames = queuedAudioFrames.get(),
        )
    }

    override fun backendLabel(options: PlaybackOptions): String {
        return if (options.preferLowLatencyPath) "audiotrack_fast_path" else "audiotrack_stable"
    }

    private fun startWriter() {
        val queue = ArrayBlockingQueue<QueuedChunk>(WRITE_QUEUE_CAPACITY_FRAMES)
        writeQueue = queue
        writerRunning = true
        val stopped = CountDownLatch(1)
        writerStoppedSignal = stopped
        Log.i(logTag, "audio writer thread started")
        writerThread = Thread({
            val threadName = Thread.currentThread().name
            val threadId = Process.myTid()
            try {
                Process.setThreadPriority(Process.THREAD_PRIORITY_AUDIO)
                val effectivePriority = Process.getThreadPriority(threadId)
                Log.i(
                    logTag,
                    "audio writer thread priority set name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) effective=$effectivePriority",
                )
            } catch (t: Throwable) {
                Log.w(
                    logTag,
                    "audio writer thread priority set failed name=$threadName tid=$threadId requested=THREAD_PRIORITY_AUDIO(${Process.THREAD_PRIORITY_AUDIO}) error=${t.message}",
                )
            }
            try {
                while (writerRunning || queue.isNotEmpty()) {
                    val track = audioTrack
                    if (track == null) {
                        Thread.sleep(5)
                        continue
                    }
                    if (!playbackStarted) {
                        if (!playRequested) {
                            Thread.sleep(5)
                            continue
                        }
                        val prefillTarget = if (transportHint == TransportHint.Usb) 3 else 8
                        val now = SystemClock.elapsedRealtime()
                        val queueSize = queue.size
                        if (lastPrefillWaitLogAtMs == 0L ||
                            now - lastPrefillWaitLogAtMs >= PREFILL_WAIT_LOG_INTERVAL_MS
                        ) {
                            Log.i(
                                logTag,
                                "prefill_wait transport=${transportHint.name.lowercase()} target=$prefillTarget queue=$queueSize",
                            )
                            lastPrefillWaitLogAtMs = now
                        }
                        if (!startupPendingLogged &&
                            playRequestedAtMs != 0L &&
                            now - playRequestedAtMs >= STARTUP_PENDING_LOG_MS
                        ) {
                            Log.w(
                                logTag,
                                "writer_start_pending transport=${transportHint.name.lowercase()} target=$prefillTarget queue=$queueSize playRequested=$playRequested playbackStarted=$playbackStarted",
                            )
                            startupPendingLogged = true
                        }
                        if (queueSize < prefillTarget) {
                            maybeLogUnderrunCount(track)
                            Thread.sleep(5)
                            continue
                        }
                        track.play()
                        playbackStarted = true
                        lastPrefillWaitLogAtMs = 0L
                        startupPendingLogged = false
                        Log.i(
                            logTag,
                            "prefill_done transport=${transportHint.name.lowercase()} queue=$queueSize",
                        )
                    }
                    val chunk = if (transportHint == TransportHint.Usb) {
                        try {
                            queue.take()
                        } catch (_: InterruptedException) {
                            break
                        }
                    } else {
                        val timeoutMs = computeAdaptiveTimeoutMs()
                        try {
                            queue.poll(timeoutMs, TimeUnit.MILLISECONDS)
                        } catch (_: InterruptedException) {
                            break
                        }
                    }
                    if (chunk == null) {
                        val t0 = System.nanoTime()
                        val wrote = track.write(silenceBuffer, 0, silenceBuffer.size, AudioTrack.WRITE_BLOCKING)
                        recordWriteCallDuration(System.nanoTime() - t0)
                        if (wrote > 0) {
                            packetLossFillCount += 1
                            writeCount.incrementAndGet()
                            maybeLogUnderrunCount(track)
                            Log.w(logTag, "wifi_packet_loss_fill timeout=${computeAdaptiveTimeoutMs()}ms")
                        }
                        continue
                    }
                    consumedCount.incrementAndGet()
                    sampleQueueSize(queue.size)
                    queuedAudioFrames.addAndGet(-chunk.frames)
                    writeFully(track, chunk)
                }
            } finally {
                stopped.countDown()
            }
        }, "lan-audio-service-track-writer").also { it.start() }
    }

    private fun stopWriter(reason: String) {
        Log.i(logTag, "audio writer thread stopping")
        writerRunning = false
        resetStartupState(reason)
        writeQueue?.clear()
        queuedAudioFrames.set(0)
        writerThread?.interrupt()
        writerStoppedSignal?.await(100, TimeUnit.MILLISECONDS)
        writerThread = null
        writeQueue = null
        writerStoppedSignal = null
        Log.i(logTag, "audio writer thread stopped")
    }

    private fun resetStartupState(reason: String) {
        if (playRequested || playbackStarted || playRequestedAtMs != 0L) {
            Log.i(
                logTag,
                "prefill_reset reason=$reason playRequested=$playRequested playbackStarted=$playbackStarted",
            )
        }
        playRequested = false
        playbackStarted = false
        playRequestedAtMs = 0L
        lastPrefillWaitLogAtMs = 0L
        startupPendingLogged = false
    }

    private fun writeFully(track: AudioTrack, chunk: QueuedChunk) {
        val data = chunk.payload
        updatePcmLevel(data)
        var offset = 0
        var shortWrite = false
        var zeroWriteRetries = 0
        while (offset < data.size) {
            val nowMs = SystemClock.elapsedRealtime()
            val previousWriteAt = lastWriteCallAtMs
            if (previousWriteAt > 0L) {
                val gapMs = nowMs - previousWriteAt
                recordWriteGap(gapMs)
                if (gapMs > WRITE_GAP_WARN_MS) {
                    Log.w(logTag, "write_gap_ms=$gapMs threshold_ms=$WRITE_GAP_WARN_MS")
                }
            }
            lastWriteCallAtMs = nowMs

            val expectedBytes = data.size - offset
            val t0 = System.nanoTime()
            val wrote = track.write(data, offset, expectedBytes, AudioTrack.WRITE_BLOCKING)
            recordWriteCallDuration(System.nanoTime() - t0)
            writeCount.incrementAndGet()
            pcmWriteCount.incrementAndGet()
            if (wrote < 0) {
                shortWriteCount += 1
                Log.w(logTag, "AudioTrack.write returned error=$wrote; drop current frame")
                return
            }
            if (wrote in 1 until expectedBytes) {
                Log.w(
                    logTag,
                    "partial_write detected wrote_bytes=$wrote expected_bytes=$expectedBytes chunk_bytes=${data.size} offset_bytes=$offset",
                )
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
        maybeLogUnderrunCount(track)
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

    private fun maybeLogUnderrunCount(track: AudioTrack) {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.N) {
            return
        }
        val now = SystemClock.elapsedRealtime()
        if (lastUnderrunSampleAtMs != 0L && now - lastUnderrunSampleAtMs < UNDERRUN_SAMPLE_INTERVAL_MS) {
            return
        }
        lastUnderrunSampleAtMs = now
        val underrunCount = try {
            track.underrunCount
        } catch (t: Throwable) {
            Log.w(logTag, "audio_track_underrun_count read failed: ${t.message}")
            null
        }
        var underrunDelta = 0
        if (underrunCount != null) {
            val previous = lastUnderrunCount
            if (previous != null) {
                underrunDelta = (underrunCount - previous).coerceAtLeast(0)
            }
            lastUnderrunCount = underrunCount
        }

        val enqueuedDelta = (enqueuedCount.get() - lastLoggedEnqueuedCount).coerceAtLeast(0)
        val enqueueDroppedDelta =
            (enqueueDroppedCount.get() - lastLoggedEnqueueDroppedCount).coerceAtLeast(0)
        val consumedDelta = (consumedCount.get() - lastLoggedConsumedCount).coerceAtLeast(0)
        val writeDelta = (writeCount.get() - lastLoggedWriteCount).coerceAtLeast(0)
        val pcmWriteDelta = (pcmWriteCount.get() - lastLoggedPcmWriteCount).coerceAtLeast(0)
        val packetLossFillTotal = packetLossFillCount
        val packetLossFillDelta = (packetLossFillTotal - lastPacketLossFillCount).coerceAtLeast(0)
        val queueStats = snapshotAndResetQueueStats()
        val writeStats = snapshotAndResetWriteStats()
        val transport = transportHint.name.lowercase()

        Log.i(
            logTag,
            String.format(
                Locale.US,
                "queue_summary interval_5s transport=%s enqueued=%d enqueue_dropped=%d consumed=%d silence_filled=%d packet_loss_fill=%d queue_size_min=%d queue_size_max=%d queue_size_avg=%.2f",
                transport,
                enqueuedDelta,
                enqueueDroppedDelta,
                consumedDelta,
                if (transportHint == TransportHint.Usb) 0 else packetLossFillDelta,
                if (transportHint == TransportHint.Wifi) packetLossFillDelta else 0,
                queueStats.min,
                queueStats.max,
                queueStats.avg,
            ),
        )
        Log.i(
            logTag,
            String.format(
                Locale.US,
                "write_summary interval_5s write_count=%d pcm_write_count=%d silence_write_count=%d write_avg_ms=%.3f write_max_ms=%.3f underrun_delta=%d",
                writeDelta,
                pcmWriteDelta,
                if (transportHint == TransportHint.Wifi) packetLossFillDelta else 0,
                writeStats.avgMs,
                writeStats.maxMs,
                underrunDelta,
            ),
        )
        maybeLogWifiRxSummary(now)
        if (underrunDelta > 0) {
            Log.w(logTag, "underrun_delta=$underrunDelta total=$underrunCount")
        }

        lastLoggedEnqueuedCount = enqueuedCount.get()
        lastLoggedEnqueueDroppedCount = enqueueDroppedCount.get()
        lastLoggedConsumedCount = consumedCount.get()
        lastLoggedWriteCount = writeCount.get()
        lastLoggedPcmWriteCount = pcmWriteCount.get()
        lastPacketLossFillCount = packetLossFillTotal
    }

    private fun maybeLogWifiRxSummary(nowMs: Long) {
        if (transportHint != TransportHint.Wifi) {
            return
        }
        if (lastWifiSummaryAtMs != 0L && nowMs - lastWifiSummaryAtMs < UNDERRUN_SAMPLE_INTERVAL_MS) {
            return
        }
        lastWifiSummaryAtMs = nowMs
        val p95 = arrivalP95Ms()
        val adaptiveTimeout = computeAdaptiveTimeoutMs()
        val packetLossFillDelta = (packetLossFillCount - lastPacketLossFillCount).coerceAtLeast(0)
        Log.i(
            logTag,
            "wifi_rx_summary interval_5s arrival_p95_ms=$p95 adaptive_timeout_ms=$adaptiveTimeout packet_loss_fill=$packetLossFillDelta",
        )
    }

    private fun recordArrivalInterval() {
        val now = SystemClock.elapsedRealtime()
        val previous = lastArrivalAtMs
        if (previous > 0L) {
            val interval = (now - previous).coerceAtLeast(0L)
            synchronized(arrivalIntervalMs) {
                if (arrivalIntervalMs.size >= ARRIVAL_WINDOW_SIZE) {
                    arrivalIntervalMs.removeFirst()
                }
                arrivalIntervalMs.addLast(interval)
            }
        }
        lastArrivalAtMs = now
    }

    private fun arrivalP95Ms(): Long {
        synchronized(arrivalIntervalMs) {
            if (arrivalIntervalMs.isEmpty()) {
                return frameDurationMs.toLong()
            }
            val sorted = arrivalIntervalMs.sorted()
            val idx = kotlin.math.ceil(sorted.size * 0.95).toInt().coerceIn(1, sorted.size) - 1
            return sorted[idx].coerceAtLeast(1L)
        }
    }

    private fun computeAdaptiveTimeoutMs(): Long {
        if (transportHint == TransportHint.Usb) {
            return frameDurationMs.toLong().coerceAtLeast(1L)
        }
        val p95 = arrivalP95Ms()
        val scaled = kotlin.math.ceil(p95.toDouble() * 1.5).toLong()
        val base = frameDurationMs.toLong().coerceAtLeast(1L)
        return maxOf(base, scaled).coerceAtMost(WIFI_TIMEOUT_MAX_MS)
    }

    private fun sampleQueueSize(size: Int) {
        synchronized(queueStatsLock) {
            queueSizeWindowMin = kotlin.math.min(queueSizeWindowMin, size)
            queueSizeWindowMax = kotlin.math.max(queueSizeWindowMax, size)
            queueSizeWindowSum += size.toLong()
            queueSizeWindowSamples += 1L
        }
    }

    private fun recordWriteCallDuration(durationNs: Long) {
        if (durationNs <= 0L) {
            return
        }
        writeDurationWindowNsSum += durationNs
        writeDurationWindowSamples += 1L
        if (durationNs > writeDurationWindowNsMax) {
            writeDurationWindowNsMax = durationNs
        }
    }

    private fun snapshotAndResetQueueStats(): QueueWindowStats {
        synchronized(queueStatsLock) {
            val min = if (queueSizeWindowSamples > 0L) queueSizeWindowMin else 0
            val max = if (queueSizeWindowSamples > 0L) queueSizeWindowMax else 0
            val avg = if (queueSizeWindowSamples > 0L) {
                queueSizeWindowSum.toDouble() / queueSizeWindowSamples.toDouble()
            } else {
                0.0
            }
            val snapshot = QueueWindowStats(min = min, max = max, avg = avg)
            queueSizeWindowMin = Int.MAX_VALUE
            queueSizeWindowMax = 0
            queueSizeWindowSum = 0L
            queueSizeWindowSamples = 0L
            return snapshot
        }
    }

    private fun snapshotAndResetWriteStats(): WriteWindowStats {
        val samples = writeDurationWindowSamples
        val avgMs = if (samples > 0L) {
            (writeDurationWindowNsSum.toDouble() / samples.toDouble()) / 1_000_000.0
        } else {
            0.0
        }
        val maxMs = writeDurationWindowNsMax.toDouble() / 1_000_000.0
        writeDurationWindowNsSum = 0L
        writeDurationWindowSamples = 0L
        writeDurationWindowNsMax = 0L
        return WriteWindowStats(avgMs = avgMs, maxMs = maxMs)
    }

    private fun buildAudioTrack(
        sampleRate: Int,
        channelConfig: Int,
        encoding: Int,
        minBufferSizeBytes: Int,
    ): AudioTrack {
        val desiredBuffer = minBufferSizeBytes * DEFAULT_BUFFER_MULTIPLIER
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
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            builder.setPerformanceMode(AudioTrack.PERFORMANCE_MODE_NONE)
        }
        return builder.build()
    }

    private fun configuredBufferSizeBytes(
        track: AudioTrack,
        channelCount: Int,
        bytesPerSample: Int,
        fallbackBytes: Int,
    ): Int {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.N) {
            val frames = track.bufferCapacityInFrames
            if (frames > 0) {
                return frames * channelCount.coerceAtLeast(1) * bytesPerSample.coerceAtLeast(1)
            }
        }
        return fallbackBytes
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

    private fun recordWriteGap(gapMs: Long) {
        synchronized(writeGapWindowMs) {
            if (writeGapWindowMs.size == 128) {
                writeGapWindowMs.removeFirst()
            }
            writeGapWindowMs.addLast(gapMs.coerceAtLeast(0L))
        }
    }

    private fun writeGapP95Ms(): Int {
        val sorted = synchronized(writeGapWindowMs) {
            writeGapWindowMs.toList().sorted()
        }
        if (sorted.isEmpty()) {
            return 0
        }
        val idx = kotlin.math.ceil(sorted.size * 0.95).toInt().coerceIn(1, sorted.size) - 1
        return sorted[idx].toInt()
    }

    private data class QueuedChunk(
        val payload: ByteArray,
        val frames: Int,
    )

    private data class QueueWindowStats(
        val min: Int,
        val max: Int,
        val avg: Double,
    )

    private data class WriteWindowStats(
        val avgMs: Double,
        val maxMs: Double,
    )

    companion object {
        const val DEFAULT_REQUESTED_SAMPLE_RATE = 48_000
        private const val WRITE_QUEUE_CAPACITY_FRAMES = 20
        private const val WRITE_GAP_WARN_MS = 40L
        private const val UNDERRUN_SAMPLE_INTERVAL_MS = 5_000L
        private const val ARRIVAL_WINDOW_SIZE = 100
        private const val WIFI_TIMEOUT_MAX_MS = 100L
        private const val PREFILL_WAIT_LOG_INTERVAL_MS = 250L
        private const val STARTUP_PENDING_LOG_MS = 1_000L
        private const val DEFAULT_BUFFER_MULTIPLIER = 3

        fun queryNativeOutputSampleRate(): Int {
            return AudioTrack.getNativeOutputSampleRate(AudioManager.STREAM_MUSIC)
                .takeIf { it > 0 }
                ?: DEFAULT_REQUESTED_SAMPLE_RATE
        }

        fun preferredStreamSampleRate(): Int {
            val nativeRate = queryNativeOutputSampleRate()
            return if (isSupportedPreferredSampleRate(nativeRate)) {
                nativeRate
            } else {
                DEFAULT_REQUESTED_SAMPLE_RATE
            }
        }

        private fun isSupportedPreferredSampleRate(sampleRate: Int): Boolean {
            return sampleRate == 8_000 ||
                sampleRate == 12_000 ||
                sampleRate == 16_000 ||
                sampleRate == 24_000 ||
                sampleRate == 48_000
        }
    }
}
