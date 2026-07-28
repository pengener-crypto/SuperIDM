package com.superidm.engine

import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject

class SpeedOptimizer @Inject constructor() {

    data class SegmentMetrics(val segmentId: String, val bytesPerSecond: Long, val remainingBytes: Long)

    private val segmentSpeeds = ConcurrentHashMap<String, MutableList<Pair<Long, Long>>>()

    fun recordBytes(segmentId: String, bytes: Long) {
        val now = System.currentTimeMillis()
        val records = segmentSpeeds.getOrPut(segmentId) { mutableListOf() }
        synchronized(records) {
            records.add(Pair(now, bytes))
            val threshold = now - 3000
            records.removeAll { it.first < threshold }
        }
    }

    fun getSpeed(segmentId: String): Long {
        val records = segmentSpeeds[segmentId] ?: return 0L
        synchronized(records) {
            if (records.isEmpty()) return 0L
            val now = System.currentTimeMillis()
            val threshold = now - 3000
            var totalBytes = 0L
            records.forEach { if (it.first >= threshold) totalBytes += it.second }
            return totalBytes / 3
        }
    }

    fun getTotalSpeed(): Long {
        return segmentSpeeds.keys().toList().sumOf { getSpeed(it) }
    }

    fun getEta(totalBytes: Long, downloadedBytes: Long): Long {
        val speed = getTotalSpeed()
        if (speed <= 0) return -1L
        val remaining = totalBytes - downloadedBytes
        return remaining / speed
    }

    fun getSlowestSegments(n: Int): List<String> {
        return segmentSpeeds.keys().toList().sortedBy { getSpeed(it) }.take(n)
    }

    fun getFastestSegments(n: Int): List<String> {
        return segmentSpeeds.keys().toList().sortedByDescending { getSpeed(it) }.take(n)
    }

    fun reset(segmentId: String) {
        segmentSpeeds.remove(segmentId)
    }

    fun resetAll() {
        segmentSpeeds.clear()
    }
}
