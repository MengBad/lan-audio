package com.example.lan_audio_android_mvp

import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.sqrt

class PlaybackLoudnessNormalizer {
    private var requestedEnabled: Boolean = false
    private var mode: String = "balanced"
    private var sampleRate: Int = 48_000
    private var channels: Int = 2
    private var currentGain: Double = 1.0
    private var targetGain: Double = 1.0
    private var rampRemainingSamples: Int = 0
    private var lastAnalysisAtMs: Long = 0L
    private var consecutiveHighGainWindows: Int = 0

    fun setEnabled(enabled: Boolean) {
        requestedEnabled = enabled
        if (!isActive()) {
            resetGain()
        }
    }

    fun setMode(nextMode: String) {
        mode = PlaybackModeProfiles.normalize(nextMode)
        if (!isActive()) {
            resetGain()
        }
    }

    fun configure(sampleRate: Int, channels: Int) {
        this.sampleRate = sampleRate.coerceAtLeast(1)
        this.channels = channels.coerceAtLeast(1)
    }

    fun process(input: ByteArray, nowMs: Long): ByteArray {
        if (!isActive() || input.size < 2) {
            return input
        }
        if (lastAnalysisAtMs == 0L || nowMs - lastAnalysisAtMs >= ANALYSIS_INTERVAL_MS) {
            targetGain = computeTargetGain(input)
            rampRemainingSamples = rampSampleCount()
            lastAnalysisAtMs = nowMs
            applyHighGainHysteresis()
        }
        // Peak-Ahead Guard: scan the upcoming buffer's peak and clamp
        // targetGain so even with the in-progress ramp we never push the
        // output past the soft-clip ceiling. This protects against transients
        // that would otherwise overshoot during the 100ms ramp window.
        applyPeakAheadGuard(input)
        return applyGain(input)
    }

    fun gainDb(): Double {
        return if (!isActive()) 0.0 else 20.0 * log10(currentGain.coerceAtLeast(0.000_001))
    }

    fun isEnabled(): Boolean = requestedEnabled

    fun isActive(): Boolean {
        return requestedEnabled && mode != "low_latency"
    }

    private fun computeTargetGain(input: ByteArray): Double {
        var sumSquares = 0.0
        var samples = 0
        var index = 0
        while (index + 1 < input.size) {
            val lo = input[index].toInt() and 0xFF
            val hi = input[index + 1].toInt()
            val sample = (hi shl 8) or lo
            val normalized = sample / PCM16_FULL_SCALE
            sumSquares += normalized * normalized
            samples += 1
            index += 2
        }
        if (samples == 0 || sumSquares <= 0.0) {
            return 1.0
        }
        val rms = sqrt(sumSquares / samples.toDouble()).coerceAtLeast(0.000_001)
        return (TARGET_RMS / rms).coerceIn(MIN_GAIN, MAX_GAIN)
    }

    private fun applyGain(input: ByteArray): ByteArray {
        val out = input.copyOf()
        var index = 0
        while (index + 1 < out.size) {
            val sampleGain = nextGain()
            val lo = out[index].toInt() and 0xFF
            val hi = out[index + 1].toInt()
            val sample = (hi shl 8) or lo
            // Soft-clip using tanh-style saturation to avoid hard clipping distortion.
            // Only engage soft-clip when the scaled sample would exceed the headroom ceiling.
            val raw = sample * sampleGain
            val scaled = if (raw > SOFT_CLIP_THRESHOLD || raw < -SOFT_CLIP_THRESHOLD) {
                softClip(raw)
            } else {
                raw.toInt()
            }
            out[index] = (scaled and 0xFF).toByte()
            out[index + 1] = ((scaled shr 8) and 0xFF).toByte()
            index += 2
        }
        return out
    }

    /**
     * Attempt soft saturation instead of hard clipping.
     * Uses a simple cubic soft-clip curve that smoothly limits the signal
     * to avoid the harsh distortion of hard clipping at ±32767.
     */
    private fun softClip(sample: Double): Int {
        val limit = PCM16_FULL_SCALE * SOFT_CLIP_CEILING
        val normalized = (sample / limit).coerceIn(-1.5, 1.5)
        // Cubic soft-clip: y = x - x^3/3 for |x| <= 1.5, clamped to ±1.0
        val clipped = if (normalized > 1.0) {
            1.0
        } else if (normalized < -1.0) {
            -1.0
        } else {
            normalized - (normalized * normalized * normalized) / 4.5
        }
        return (clipped * limit).toInt().coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt())
    }

    private fun nextGain(): Double {
        if (rampRemainingSamples <= 0) {
            currentGain = targetGain
            return currentGain
        }
        val step = (targetGain - currentGain) / rampRemainingSamples.toDouble()
        currentGain += step
        rampRemainingSamples -= 1
        return currentGain
    }

    private fun rampSampleCount(): Int {
        return ((sampleRate * channels).coerceAtLeast(1) * RAMP_MS / 1000).coerceAtLeast(1)
    }

    private fun resetGain() {
        currentGain = 1.0
        targetGain = 1.0
        rampRemainingSamples = 0
        lastAnalysisAtMs = 0L
        consecutiveHighGainWindows = 0
    }

    /**
     * Peak-Ahead Guard. Scans the input buffer for its absolute peak and, if
     * the current effective gain (max of currentGain and targetGain during
     * ramp) would push that peak past the soft-clip ceiling, lowers
     * targetGain to keep the signal inside the safe range.
     */
    private fun applyPeakAheadGuard(input: ByteArray) {
        var peak = 0
        var index = 0
        while (index + 1 < input.size) {
            val lo = input[index].toInt() and 0xFF
            val hi = input[index + 1].toInt()
            val sample = (hi shl 8) or lo
            val abs = if (sample < 0) -sample else sample
            if (abs > peak) peak = abs
            index += 2
        }
        if (peak <= 0) return
        val effectiveGain = if (currentGain > targetGain) currentGain else targetGain
        val projected = peak.toDouble() * effectiveGain
        if (projected > SOFT_CLIP_THRESHOLD) {
            val safeGain = SOFT_CLIP_THRESHOLD / peak.toDouble()
            // Only lower targetGain — never raise it. If the existing target
            // is already lower than safeGain we leave it alone.
            if (safeGain < targetGain) {
                targetGain = safeGain.coerceAtLeast(MIN_GAIN)
            }
        }
    }

    /**
     * Hysteresis on persistently high target gain. If the analyser repeatedly
     * asks for >= HIGH_GAIN_THRESHOLD across HIGH_GAIN_HOLD_WINDOWS analysis
     * windows, cap targetGain so we don't accumulate distortion from
     * over-amplifying already-loud material.
     */
    private fun applyHighGainHysteresis() {
        if (targetGain >= HIGH_GAIN_THRESHOLD) {
            consecutiveHighGainWindows += 1
        } else {
            consecutiveHighGainWindows = 0
        }
        if (consecutiveHighGainWindows >= HIGH_GAIN_HOLD_WINDOWS) {
            targetGain = targetGain.coerceAtMost(HIGH_GAIN_CAP)
        }
    }

    companion object {
        private const val ANALYSIS_INTERVAL_MS = 500L
        private const val RAMP_MS = 100
        private const val PCM16_FULL_SCALE = 32768.0
        private val TARGET_RMS = 10.0.pow(-18.0 / 20.0)
        private const val MIN_GAIN = 0.5
        private const val MAX_GAIN = 1.6
        // Soft-clip engages when sample magnitude exceeds this fraction of full scale
        private const val SOFT_CLIP_THRESHOLD = 28_000.0
        // Maximum output level as fraction of full scale (leaves ~1.2 dB headroom)
        private const val SOFT_CLIP_CEILING = 0.92
        // High-gain hysteresis: if the analyser keeps asking for >= 1.4x for
        // 3 consecutive windows, cap the target at 1.3x to avoid stacking
        // headroom-eating gain on already-loud material.
        private const val HIGH_GAIN_THRESHOLD = 1.4
        private const val HIGH_GAIN_CAP = 1.3
        private const val HIGH_GAIN_HOLD_WINDOWS = 3
    }
}
