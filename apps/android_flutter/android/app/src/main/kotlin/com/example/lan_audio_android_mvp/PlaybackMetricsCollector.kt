package com.example.lan_audio_android_mvp

internal class PlaybackMetricsCollector(
    private val smoothingAlpha: Double = 0.25,
) {
    var smoothedRxFramesPerSec: Double? = null
        private set

    fun reset() {
        smoothedRxFramesPerSec = null
    }

    fun observeRxFramesPerSec(observedRxFramesPerSec: Double): Double {
        val smoothed = smoothedRxFramesPerSec?.let { previous ->
            previous + ((observedRxFramesPerSec - previous) * smoothingAlpha)
        } ?: observedRxFramesPerSec
        smoothedRxFramesPerSec = smoothed
        return smoothed
    }
}
