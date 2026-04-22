package com.example.lan_audio_android_mvp

import android.os.Handler
import android.os.Looper
import io.flutter.plugin.common.EventChannel

object PlaybackEventBus {
    private val lock = Any()
    private val mainHandler = Handler(Looper.getMainLooper())
    private var sink: EventChannel.EventSink? = null
    private var latest: Map<String, Any?> = PlaybackSnapshot().toStableServiceSnapshot().toMap()

    fun attachSink(eventSink: EventChannel.EventSink) {
        val snapshot: Map<String, Any?>
        synchronized(lock) {
            sink = eventSink
            snapshot = latest
        }
        dispatchOnMainThread(eventSink, snapshot)
    }

    fun detachSink() {
        synchronized(lock) {
            sink = null
        }
    }

    fun publish(snapshot: PlaybackSnapshot) {
        val payload = snapshot.toStableServiceSnapshot().toMap()
        val currentSink: EventChannel.EventSink?
        synchronized(lock) {
            latest = payload
            currentSink = sink
        }
        if (currentSink != null) {
            dispatchOnMainThread(currentSink, payload)
        }
    }

    fun snapshotMap(): Map<String, Any?> {
        synchronized(lock) {
            return latest
        }
    }

    private fun dispatchOnMainThread(targetSink: EventChannel.EventSink, payload: Map<String, Any?>) {
        mainHandler.post {
            try {
                targetSink.success(payload)
            } catch (_: Throwable) {
            }
        }
    }
}
