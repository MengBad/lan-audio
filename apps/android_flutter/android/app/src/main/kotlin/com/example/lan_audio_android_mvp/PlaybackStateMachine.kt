package com.example.lan_audio_android_mvp

import android.util.Log

enum class PlaybackSessionState {
    IDLE,
    CONNECTING,
    PLAYING,
    STOPPING,
    ERROR,
}

class PlaybackStateMachine(
    private val logTag: String = "lan_audio_state",
) {
    var state: PlaybackSessionState = PlaybackSessionState.IDLE
        private set

    fun transitionTo(next: PlaybackSessionState, reason: String): Boolean {
        val previous = state
        if (!isAllowed(previous, next)) {
            Log.w(logTag, "invalid playback state transition $previous -> $next reason=$reason")
            return false
        }
        if (previous != next) {
            Log.i(logTag, "playback state transition $previous -> $next reason=$reason")
        }
        state = next
        return true
    }

    private fun isAllowed(
        previous: PlaybackSessionState,
        next: PlaybackSessionState,
    ): Boolean {
        if (previous == next) {
            return true
        }
        return when (previous) {
            PlaybackSessionState.IDLE ->
                next == PlaybackSessionState.CONNECTING || next == PlaybackSessionState.ERROR
            PlaybackSessionState.CONNECTING ->
                next == PlaybackSessionState.PLAYING ||
                    next == PlaybackSessionState.STOPPING ||
                    next == PlaybackSessionState.ERROR
            PlaybackSessionState.PLAYING ->
                next == PlaybackSessionState.CONNECTING ||
                    next == PlaybackSessionState.STOPPING ||
                    next == PlaybackSessionState.ERROR
            PlaybackSessionState.STOPPING ->
                next == PlaybackSessionState.IDLE || next == PlaybackSessionState.ERROR
            PlaybackSessionState.ERROR ->
                next == PlaybackSessionState.CONNECTING ||
                    next == PlaybackSessionState.STOPPING ||
                    next == PlaybackSessionState.IDLE
        }
    }
}
