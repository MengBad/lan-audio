package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

data class AudioTrackStats(
    val nativeQueuedFrames: Int,
    val audioTrackWriteFrames: Long,
    val audioTrackShortWriteCount: Long,
)

class AudioTrackController {
    private var audioTrack: AudioTrack? = null
    private var frameBytesPerPacket: Int = 1920
    private var writeQueue: ArrayBlockingQueue<ByteArray>? = null

    @Volatile
    private var writerRunning: Boolean = false

    @Volatile
    private var writeFrames: Long = 0

    @Volatile
    private var shortWriteCount: Long = 0

    private var writerThread: Thread? = null
    @Volatile
    private var writerStoppedSignal: CountDownLatch? = null

    fun init(sampleRate: Int, channels: Int, frameSamplesPerChannel: Int) {
        stopWriter()
        audioTrack?.release()
        writeFrames = 0
        shortWriteCount = 0

        val channelConfig = if (channels == 1) {
            AudioFormat.CHANNEL_OUT_MONO
        } else {
            AudioFormat.CHANNEL_OUT_STEREO
        }

        val minBuffer = AudioTrack.getMinBufferSize(
            sampleRate,
            channelConfig,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        require(minBuffer > 0) { "AudioTrack.getMinBufferSize failed: $minBuffer" }

        val frameBytes = frameSamplesPerChannel * channels * 2
        frameBytesPerPacket = frameBytes
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

    fun start() {
        audioTrack?.play() ?: throw IllegalStateException("AudioTrack is not initialized")
    }

    fun writePcm16(data: ByteArray) {
        val track = audioTrack ?: throw IllegalStateException("AudioTrack is not initialized")
        val queue = writeQueue ?: throw IllegalStateException("AudioTrack writer is not initialized")
        if (track.playState != AudioTrack.PLAYSTATE_PLAYING) {
            track.play()
        }
        val copy = data.copyOf()
        if (!queue.offer(copy)) {
            queue.poll()
            queue.offer(copy)
        }
    }

    fun stop() {
        writeQueue?.clear()
        audioTrack?.pause()
        audioTrack?.flush()
    }

    fun release() {
        stopWriter()
        audioTrack?.release()
        audioTrack = null
    }

    fun stats(): AudioTrackStats {
        return AudioTrackStats(
            nativeQueuedFrames = writeQueue?.size ?: 0,
            audioTrackWriteFrames = writeFrames,
            audioTrackShortWriteCount = shortWriteCount,
        )
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
        }, "lan-audio-service-track-writer").also { it.start() }
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
}
