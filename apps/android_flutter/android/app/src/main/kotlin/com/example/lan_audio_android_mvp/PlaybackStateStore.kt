package com.example.lan_audio_android_mvp

class PlaybackStateStore {
    private val lock = Any()
    private var snapshot: PlaybackSnapshot = PlaybackSnapshot()
    private val listeners = LinkedHashSet<(PlaybackSnapshot) -> Unit>()

    fun current(): PlaybackSnapshot {
        synchronized(lock) {
            return snapshot
        }
    }

    fun set(next: PlaybackSnapshot) {
        val callbacks: List<(PlaybackSnapshot) -> Unit>
        synchronized(lock) {
            snapshot = next
            callbacks = listeners.toList()
        }
        PlaybackEventBus.publish(next)
        callbacks.forEach { it(next) }
    }

    fun update(block: (PlaybackSnapshot) -> PlaybackSnapshot) {
        val next: PlaybackSnapshot
        val callbacks: List<(PlaybackSnapshot) -> Unit>
        synchronized(lock) {
            next = block(snapshot)
            snapshot = next
            callbacks = listeners.toList()
        }
        PlaybackEventBus.publish(next)
        callbacks.forEach { it(next) }
    }

    fun addListener(listener: (PlaybackSnapshot) -> Unit) {
        synchronized(lock) {
            listeners.add(listener)
        }
    }

    fun removeListener(listener: (PlaybackSnapshot) -> Unit) {
        synchronized(lock) {
            listeners.remove(listener)
        }
    }
}
