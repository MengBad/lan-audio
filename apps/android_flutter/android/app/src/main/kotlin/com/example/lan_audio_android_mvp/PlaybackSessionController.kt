package com.example.lan_audio_android_mvp

import android.content.Context

/**
 * Thin service-facing coordinator for playback sessions.
 *
 * Heavy runtime ownership lives in PlaybackSessionRuntime so this class stays
 * small and does not touch jitter/audio sink internals directly.
 */
class PlaybackSessionController(
    context: Context,
    stateStore: PlaybackStateStore,
) {
    private val runtime = PlaybackSessionRuntime(context, stateStore)
    private val stateMachine = PlaybackStateMachine()

    fun startPlayback(target: PlaybackTarget) {
        stateMachine.transitionTo(PlaybackSessionState.CONNECTING, "start:${target.transportMode}")
        runtime.startPlayback(target)
    }

    fun stopPlayback(reason: String = "user_stop") {
        stateMachine.transitionTo(PlaybackSessionState.STOPPING, reason)
        runtime.stopPlayback(reason)
        stateMachine.transitionTo(PlaybackSessionState.IDLE, reason)
    }

    fun reconnect(reason: String = "manual_reconnect") {
        stateMachine.transitionTo(PlaybackSessionState.CONNECTING, reason)
        runtime.reconnect(reason)
    }

    fun hasActiveTarget(): Boolean {
        return runtime.hasActiveTarget()
    }

    fun setOptions(next: PlaybackOptions) {
        runtime.setOptions(next)
    }

    fun setAudioMode(
        mode: String,
        reason: String = "user_selected",
        preferredCodec: String? = null,
    ) {
        runtime.setAudioMode(mode, reason, preferredCodec)
    }

    fun setEqSettings(settings: PlaybackEqSettings) {
        runtime.setEqSettings(settings)
    }

    fun setLoudnessNormalization(enabled: Boolean) {
        runtime.setLoudnessNormalization(enabled)
    }

    fun dumpMetrics(reason: String = "manual_request") {
        runtime.dumpMetrics(reason)
    }

    fun destroy() {
        stateMachine.transitionTo(PlaybackSessionState.STOPPING, "destroy")
        runtime.destroy()
        stateMachine.transitionTo(PlaybackSessionState.IDLE, "destroy")
    }
}
