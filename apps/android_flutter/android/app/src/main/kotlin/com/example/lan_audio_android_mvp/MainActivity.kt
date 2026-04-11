package com.example.lan_audio_android_mvp

import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import androidx.annotation.NonNull
import io.flutter.embedding.android.FlutterActivity
import io.flutter.embedding.engine.FlutterEngine
import io.flutter.plugin.common.MethodCall
import io.flutter.plugin.common.MethodChannel

class MainActivity : FlutterActivity() {
    private val channelName = "lan_audio/audio_track"
    private var audioTrack: AudioTrack? = null
    private var sampleRate: Int = 48000
    private var channels: Int = 2

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
                val track = audioTrack ?: throw IllegalStateException("AudioTrack is not initialized")
                track.write(data, 0, data.size, AudioTrack.WRITE_BLOCKING)
                result.success(null)
            }

            "stop" -> {
                audioTrack?.pause()
                audioTrack?.flush()
                result.success(null)
            }

            "release" -> {
                audioTrack?.release()
                audioTrack = null
                result.success(null)
            }

            else -> result.notImplemented()
        }
    }

    private fun initAudioTrack(sr: Int, ch: Int, frameSamplesPerChannel: Int) {
        audioTrack?.release()

        sampleRate = sr
        channels = ch

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
        val desiredBuffer = maxOf(minBuffer, frameBytes * 6)

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
    }
}
