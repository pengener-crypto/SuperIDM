package com.superidm.scheduler

import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import java.util.Calendar

data class TimeWindow(
    val startHour: Int,
    val startMinute: Int,
    val endHour: Int,
    val endMinute: Int,
    val maxBytesPerSec: Long
)

class BandwidthLimiter {
    val windows: MutableStateFlow<List<TimeWindow>> = MutableStateFlow(emptyList())

    fun getCurrentLimit(): Long {
        val calendar = Calendar.getInstance()
        val currentHour = calendar.get(Calendar.HOUR_OF_DAY)
        val currentMinute = calendar.get(Calendar.MINUTE)
        
        var mostRestrictive = Long.MAX_VALUE
        var found = false
        
        for (window in windows.value) {
            if (isInWindow(window, currentHour, currentMinute)) {
                if (window.maxBytesPerSec < mostRestrictive) {
                    mostRestrictive = window.maxBytesPerSec
                    found = true
                }
            }
        }
        
        return if (found) mostRestrictive else 0L
    }

    private fun isInWindow(window: TimeWindow, currentHour: Int, currentMinute: Int): Boolean {
        val currentTotalMinutes = currentHour * 60 + currentMinute
        val startTotalMinutes = window.startHour * 60 + window.startMinute
        val endTotalMinutes = window.endHour * 60 + window.endMinute
        
        return if (startTotalMinutes <= endTotalMinutes) {
            currentTotalMinutes in startTotalMinutes..endTotalMinutes
        } else {
            currentTotalMinutes >= startTotalMinutes || currentTotalMinutes <= endTotalMinutes
        }
    }

    fun isInWindow(window: TimeWindow): Boolean {
        val calendar = Calendar.getInstance()
        val currentHour = calendar.get(Calendar.HOUR_OF_DAY)
        val currentMinute = calendar.get(Calendar.MINUTE)
        return isInWindow(window, currentHour, currentMinute)
    }

    fun addWindow(window: TimeWindow) {
        val current = windows.value.toMutableList()
        current.add(window)
        windows.value = current
    }

    fun removeWindow(index: Int) {
        val current = windows.value.toMutableList()
        if (index in current.indices) {
            current.removeAt(index)
            windows.value = current
        }
    }

    suspend fun throttle(bytesJustWritten: Long, startTime: Long) {
        val limit = getCurrentLimit()
        if (limit > 0) {
            val expectedTimeMs = (bytesJustWritten * 1000) / limit
            val elapsedTimeMs = System.currentTimeMillis() - startTime
            val diff = expectedTimeMs - elapsedTimeMs
            if (diff > 0) {
                delay(diff)
            }
        }
    }
}
