package com.example.lan_audio_android_mvp

import android.media.AudioFormat
import android.media.AudioRecord
import android.media.MediaRecorder
import android.util.Log
import java.io.OutputStream
import java.net.Socket
import java.util.concurrent.atomic.AtomicBoolean

class MicCaptureService(
    private val host: String,
    private val reversePort: Int,
    private val onLevel: (peakDb: Float, rmsDb: Float) -> Unit,
    private val onError: (String) -> Unit
) {
    companion object {
        const val TAG = "MicCaptureService"
        const val SAMPLE_RATE = 48000
        const val CHANNELS = AudioFormat.CHANNEL_IN_MONO
        const val ENCODING = AudioFormat.ENCODING_PCM_16BIT
        const val FRAME_SIZE_MS = 20
        const val SAMPLES_PER_FRAME = SAMPLE_RATE * FRAME_SIZE_MS / 1000 // 960
        const val BITRATE_BPS = 64000

        init {
            System.loadLibrary("lan_audio_opus_jni")
        }
    }

    private val running = AtomicBoolean(false)
    private var recordThread: Thread? = null
    private var socket: Socket? = null
    private var opusEncoder: Long = 0
    private var outputStream: OutputStream? = null

    fun start() {
        if (running.getAndSet(true)) return
        recordThread = Thread {
            try {
                captureLoop()
            } catch (e: Exception) {
                Log.e(TAG, "Mic capture failed", e)
                onError(e.message ?: "Unknown mic error")
            } finally {
                running.set(false)
            }
        }.apply {
            name = "mic-capture"
            priority = Thread.MAX_PRIORITY
            start()
        }
    }

    fun stop() {
        running.set(false)
        recordThread?.interrupt()
        recordThread = null
        try {
            socket?.close()
        } catch (_: Exception) {}
        socket = null
        outputStream = null
        if (opusEncoder != 0L) {
            nativeOpusEncoderDestroy(opusEncoder)
            opusEncoder = 0
        }
    }

    val isRunning: Boolean get() = running.get()

    private fun captureLoop() {
        val bufferSize = AudioRecord.getMinBufferSize(SAMPLE_RATE, CHANNELS, ENCODING)
        if (bufferSize <= 0) {
            onError("AudioRecord.getMinBufferSize failed: $bufferSize")
            return
        }
        val recorder = AudioRecord(
            MediaRecorder.AudioSource.MIC,
            SAMPLE_RATE,
            CHANNELS,
            ENCODING,
            bufferSize * 2
        )
        if (recorder.state != AudioRecord.STATE_INITIALIZED) {
            recorder.release()
            onError("AudioRecord failed to initialize")
            return
        }

        try {
            socket = Socket(host, reversePort)
            outputStream = socket!!.getOutputStream()
            opusEncoder = nativeOpusEncoderCreate(SAMPLE_RATE, 1, BITRATE_BPS)
            if (opusEncoder == 0L) {
                onError("Opus encoder creation failed")
                return
            }

            recorder.startRecording()
            val pcmBuffer = ShortArray(SAMPLES_PER_FRAME)
            val opusBuffer = ByteArray(4096)

            while (running.get()) {
                val read = recorder.read(pcmBuffer, 0, SAMPLES_PER_FRAME)
                if (read <= 0) continue

                var peak = 0f
                var sumSq = 0f
                for (i in 0 until read) {
                    val s = pcmBuffer[i].toFloat() / 32768f
                    val abs = Math.abs(s)
                    if (abs > peak) peak = abs
                    sumSq += s * s
                }
                val rms = Math.sqrt((sumSq / read).toDouble()).toFloat()
                val peakDb = if (peak > 0f) (20.0 * Math.log10(peak.toDouble())).toFloat() else -96f
                val rmsDb = if (rms > 0f) (20.0 * Math.log10(rms.toDouble())).toFloat() else -96f
                onLevel(peakDb, rmsDb)

                val encodedLen = nativeOpusEncode(
                    opusEncoder, pcmBuffer, read, opusBuffer, opusBuffer.size
                )
                if (encodedLen > 0) {
                    val out = outputStream ?: break
                    out.write(
                        byteArrayOf(
                            (encodedLen and 0xFF).toByte(),
                            ((encodedLen shr 8) and 0xFF).toByte(),
                            ((encodedLen shr 16) and 0xFF).toByte(),
                            ((encodedLen shr 24) and 0xFF).toByte()
                        )
                    )
                    out.write(opusBuffer, 0, encodedLen)
                    out.flush()
                }
            }
        } finally {
            recorder.stop()
            recorder.release()
            try {
                socket?.close()
            } catch (_: Exception) {}
            socket = null
            outputStream = null
            if (opusEncoder != 0L) {
                nativeOpusEncoderDestroy(opusEncoder)
                opusEncoder = 0
            }
        }
    }

    private external fun nativeOpusEncoderCreate(sampleRate: Int, channels: Int, bitrate: Int): Long
    private external fun nativeOpusEncode(
        encoder: Long, pcm: ShortArray, samples: Int, output: ByteArray, maxOutput: Int
    ): Int
    private external fun nativeOpusEncoderDestroy(encoder: Long)
}
