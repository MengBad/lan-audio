package com.example.lan_audio_android_mvp

/**
 * Phase 6 v3 PCM24 fragmentation reassembler.
 *
 * The server may split a single logical PCM24 frame into 1..N UDP packets
 * (typically 1 at 48 kHz, 2 at 96 kHz). Each packet carries
 * `(logical_seq, frag_index, total_frags)` in its v3 header. This class
 * collects packets keyed on `logical_seq` and emits a single rebuilt
 * `LasPacket` once all frags for a logical_seq have arrived.
 *
 * Bounded buffer: we keep at most 8 in-progress logical_seq slots. Older
 * slots are dropped silently if a 9th arrives — we expect any frag drop
 * to be detected by the receiver's normal jitter logic, not here.
 */
class LasPacketReassembler {
    private data class Slot(
        val totalFrags: Int,
        val frags: Array<ByteArray?>,
        val templates: Array<LasPacket?>,
        var receivedCount: Int,
    )

    private val slots = LinkedHashMap<Long, Slot>(MAX_SLOTS, 0.75f, true)

    /**
     * Feed one wire packet to the reassembler.
     *
     * - For v1 / v2 packets (`isFragmented == false`) returns the packet
     *   unchanged.
     * - For v3 single-frag packets (totalFrags == 1) returns the packet
     *   unchanged.
     * - For v3 multi-frag packets, returns the rebuilt `LasPacket` once
     *   all frags for the same `logical_seq` have arrived; otherwise
     *   returns null and waits for more.
     */
    fun feed(packet: LasPacket): LasPacket? {
        if (!packet.isFragmented) {
            return packet
        }
        val key = packet.logicalSeq
        val slot = slots.getOrPut(key) {
            Slot(
                totalFrags = packet.totalFrags,
                frags = arrayOfNulls(packet.totalFrags),
                templates = arrayOfNulls(packet.totalFrags),
                receivedCount = 0,
            )
        }
        // Defensive: server must keep totalFrags consistent across all
        // frags of the same logical_seq. If it ever differs we drop the
        // slot to avoid emitting a corrupt buffer.
        if (slot.totalFrags != packet.totalFrags) {
            slots.remove(key)
            return null
        }
        if (packet.fragIndex < 0 || packet.fragIndex >= slot.totalFrags) {
            return null
        }
        if (slot.frags[packet.fragIndex] == null) {
            slot.frags[packet.fragIndex] = packet.payload
            slot.templates[packet.fragIndex] = packet
            slot.receivedCount += 1
        }

        if (slot.receivedCount < slot.totalFrags) {
            evictExcess()
            return null
        }

        // Complete: glue the chunks in frag-index order.
        var totalSize = 0
        for (chunk in slot.frags) {
            totalSize += chunk?.size ?: return null
        }
        val combined = ByteArray(totalSize)
        var cursor = 0
        for (chunk in slot.frags) {
            val src = chunk ?: return null
            System.arraycopy(src, 0, combined, cursor, src.size)
            cursor += src.size
        }
        slots.remove(key)

        // Use the first frag's header as the template — sample rate /
        // codec / timestamp are guaranteed identical across frags by
        // the server.
        val template = slot.templates[0] ?: packet
        return LasPacket(
            protocolVersion = template.protocolVersion,
            headerSize = template.headerSize,
            flags = template.flags,
            sequence = template.sequence,
            timestampMs = template.timestampMs,
            codec = template.codec,
            sampleRate = template.sampleRate,
            channels = template.channels,
            framesPerPacket = template.framesPerPacket,
            payload = combined,
            fragIndex = 0,
            totalFrags = 1,
            logicalSeq = key,
        )
    }

    /** Reset reassembler state — typically called on stream restart. */
    fun reset() {
        slots.clear()
    }

    private fun evictExcess() {
        while (slots.size > MAX_SLOTS) {
            val it = slots.entries.iterator()
            if (it.hasNext()) {
                it.next()
                it.remove()
            }
        }
    }

    companion object {
        const val MAX_SLOTS = 8
    }
}
