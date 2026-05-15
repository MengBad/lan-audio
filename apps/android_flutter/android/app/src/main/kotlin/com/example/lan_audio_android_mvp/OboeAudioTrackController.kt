package com.example.lan_audio_android_mvp

import android.media.AudioFormat
import android.media.AudioTrack
import android.os.Build
import android.os.SystemClock
import android.util.Log
import java.nio.ByteBuffer
import java.nio.ByteOrder
import java.util.Locale

class OboeAudioTrackController : PlaybackAudioSink {
    private val logTag = "lan_audio_oboe"
    private var sampleRate: Int = 48_000
    private var channelCount: Int = 2
    private var frameSamplesPerChannel: Int = 960
    private var transportHint: TransportHint = TransportHint.Wifi
    private var audioEncoding: Int = AudioFormat.ENCODING_PCM_16BIT
    private var opened = false
    private var started = false
    private var pushedFrames: Long = 0
    private var pushFailures: Long = 0
    private var lastPcmPeak: Int = 0
    private var lastPcmRms: Double = 0.0
    private var lastSummaryAtMs: Long = 0
    private var lastLoggedSilenceFill: Long = 0
    private var lastLoggedUnderrun: Int = 0

    override fun init(
        sampleRate: Int,
        channels: Int,
        frameSamplesPerChannel: Int,
        transportHint: TransportHint,
        encoding: Int,
    ) {
        require(encoding == AudioFormat.ENCODING_PCM_16BIT) { "Oboe sink currently supports PCM16 only" }
        check(OpusNativeDecoder.isAvailable()) { "shared JNI library is not available" }
        release()
        this.sampleRate = sampleRate
        this.channelCount = channels.coerceAtLeast(1)
        this.frameSamplesPerChannel = frameSamplesPerChannel.coerceAtLeast(1)
        this.transportHint = transportHint
        this.audioEncoding = encoding
        pushedFrames = 0
        pushFailures = 0
        lastPcmPeak = 0
        lastPcmRms = 0.0
        lastSummaryAtMs = 0L
        lastLoggedSilenceFill = 0L
        lastLoggedUnderrun = 0
        val ok = nativeOpen(this.sampleRate, this.channelCount)
        opened = ok
        Log.i(
            logTag,
            "oboe_open ok=$ok sr=${this.sampleRate} ch=${this.channelCount} sdk=${Build.VERSION.SDK_INT} transport=${transportHint.name.lowercase()}",
        )
        check(ok) { "failed to open Oboe sink" }
    }

    override fun start() {
        started = true
    }

    override fun setQueueSoftCapFrames(maxQueuedFrames: Int) {
        // Native Oboe path uses a fixed ring buffer capacity for now.
    }

    override fun setEqSettings(settings: PlaybackEqSettings) {
        if (!opened) {
            // The native sink is not yet initialized. Settings will be
            // re-applied once `init` runs because the Kotlin sink is
            // recreated and `setEqSettings` is called from
            // `PlaybackSessionRuntime.openPlaybackSink`.
            Log.i(
                logTag,
                "set_eq_pre_open enabled=${settings.enabled} low=${settings.lowDb} mid=${settings.midDb} high=${settings.highDb}",
            )
            return
        }
        val clamped = settings.clamped()
        nativeSetEqSettings(clamped.enabled, clamped.lowDb, clamped.midDb, clamped.highDb)
        Log.i(
            logTag,
            "set_eq_native enabled=${clamped.enabled} low=${clamped.lowDb} mid=${clamped.midDb} high=${clamped.highDb}",
        )
    }

    override fun writePcm16(data: ByteArray, frames: Int) {
        check(opened) { "Oboe sink is not initialized" }
        val safeFrames = frames.coerceAtLeast(1)
        val expectedBytes = safeFrames * channelCount.coerceAtLeast(1) * 2
        if (data.size != expectedBytes) {
            pushFailures += 1
            Log.e(
                logTag,
                "oboe_push_size_mismatch bytes=${data.size} expected=$expectedBytes frames=$safeFrames channels=$channelCount",
            )
            return
        }
        updatePcmLevel(data)
        val ok = nativePushPcm(data, safeFrames)
        if (ok) {
            pushedFrames += safeFrames.toLong()
        } else {
            pushFailures += 1
            Log.w(
                logTag,
                "oboe_push_backpressure frames=$safeFrames bytes=${data.size} ring_level=${nativeGetRingBufferLevelFrames()}",
            )
        }
    }

    override fun stop() {
        started = false
    }

    override fun release() {
        if (opened) {
            nativeClose()
        }
        opened = false
        started = false
    }

    override fun stats(): PlaybackAudioSinkStats {
        if (!opened) {
            return PlaybackAudioSinkStats(
                nativeQueuedFrames = 0,
                nativeQueuedAudioFrames = 0,
                audioTrackWriteFrames = pushedFrames,
                audioTrackShortWriteCount = pushFailures,
                writeGapP95Ms = 0,
                reportedLatencyMs = null,
                lastPcmPeak = lastPcmPeak,
                lastPcmRms = lastPcmRms,
                lastPlayState = if (started) AudioTrack.PLAYSTATE_PLAYING else AudioTrack.PLAYSTATE_STOPPED,
                silenceFillTotal = 0,
                underrunTotal = 0,
                ringBufferLevelFrames = 0,
            )
        }
        val ringFrames = nativeGetRingBufferLevelFrames().coerceAtLeast(0)
        val queuedPackets = if (frameSamplesPerChannel > 0) {
            kotlin.math.ceil(ringFrames.toDouble() / frameSamplesPerChannel.toDouble()).toInt()
        } else {
            0
        }
        val silenceFillTotal = nativeGetSilenceFill().toLong().coerceAtLeast(0L)
        val underrunTotal = nativeGetUnderrunCount().coerceAtLeast(0)
        maybeLogSummary(silenceFillTotal, underrunTotal, ringFrames)
        return PlaybackAudioSinkStats(
            nativeQueuedFrames = queuedPackets,
            nativeQueuedAudioFrames = queuedPackets,
            audioTrackWriteFrames = pushedFrames,
            audioTrackShortWriteCount = pushFailures,
            writeGapP95Ms = 0,
            reportedLatencyMs = framesToMs(ringFrames),
            lastPcmPeak = lastPcmPeak,
            lastPcmRms = lastPcmRms,
            lastPlayState = if (started) AudioTrack.PLAYSTATE_PLAYING else AudioTrack.PLAYSTATE_STOPPED,
            silenceFillTotal = silenceFillTotal,
            underrunTotal = underrunTotal,
            ringBufferLevelFrames = ringFrames,
        )
    }

    override fun backendLabel(options: PlaybackOptions): String {
        return if (options.preferLowLatencyPath) "oboe_callback_low_latency" else "oboe_callback"
    }

    private fun maybeLogSummary(silenceFillTotal: Long, underrunTotal: Int, ringFrames: Int) {
        val now = SystemClock.elapsedRealtime()
        if (lastSummaryAtMs != 0L && now - lastSummaryAtMs < SUMMARY_INTERVAL_MS) {
            return
        }
        val silenceDelta = (silenceFillTotal - lastLoggedSilenceFill).coerceAtLeast(0L)
        val underrunDelta = (underrunTotal - lastLoggedUnderrun).coerceAtLeast(0)
        Log.i(
            logTag,
            String.format(
                Locale.US,
                "oboe_summary interval_5s silence_fill_delta=%d underrun_delta=%d ring_buf_level=%d transport=%s",
                silenceDelta,
                underrunDelta,
                ringFrames,
                transportHint.name.lowercase(),
            ),
        )
        lastSummaryAtMs = now
        lastLoggedSilenceFill = silenceFillTotal
        lastLoggedUnderrun = underrunTotal
    }

    private fun framesToMs(frames: Int): Int {
        return ((frames.coerceAtLeast(0).toLong() * 1000L) / sampleRate.coerceAtLeast(1).toLong()).toInt()
    }

    private fun updatePcmLevel(data: ByteArray) {
        if (data.size < 2) {
            lastPcmPeak = 0
            lastPcmRms = 0.0
            return
        }
        val buffer = ByteBuffer.wrap(data).order(ByteOrder.LITTLE_ENDIAN)
        var peak = 0
        var sumSquares = 0.0
        var samples = 0
        while (buffer.remaining() >= 2) {
            val sample = buffer.short.toInt()
            peak = maxOf(peak, kotlin.math.abs(sample))
            val normalized = sample / 32768.0
            sumSquares += normalized * normalized
            samples += 1
        }
        lastPcmPeak = peak
        lastPcmRms = if (samples == 0) 0.0 else kotlin.math.sqrt(sumSquares / samples)
    }

    private external fun nativeOpen(sampleRate: Int, channelCount: Int): Boolean
    private external fun nativeClose()
    private external fun nativePushPcm(pcmBytes: ByteArray, frames: Int): Boolean
    private external fun nativeGetSilenceFill(): Int
    private external fun nativeGetUnderrunCount(): Int
    private external fun nativeGetRingBufferLevelFrames(): Int
    private external fun nativeSetEqSettings(enabled: Boolean, lowDb: Int, midDb: Int, highDb: Int)

    companion object {
        private const val SUMMARY_INTERVAL_MS = 5_000L

        init {
            OpusNativeDecoder.isAvailable()
        }
    }
}
