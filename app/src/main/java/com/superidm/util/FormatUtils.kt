package com.superidm.util

import kotlin.math.ln
import kotlin.math.pow

object FormatUtils {
    fun formatSpeed(bytesPerSecond: Long): String {
        if (bytesPerSecond < 1024) return "$bytesPerSecond B/s"
        val exp = (ln(bytesPerSecond.toDouble()) / ln(1024.0)).toInt()
        val pre = "KMGTPE"[exp - 1] + ""
        return String.format("%.1f %sB/s", bytesPerSecond / 1024.0.pow(exp.toDouble()), pre)
    }

    fun formatSize(bytes: Long): String {
        if (bytes < 1024) return "$bytes B"
        val exp = (ln(bytes.toDouble()) / ln(1024.0)).toInt()
        val pre = "KMGTPE"[exp - 1] + ""
        return String.format("%.1f %sB", bytes / 1024.0.pow(exp.toDouble()), pre)
    }

    fun formatEta(seconds: Long): String {
        if (seconds < 0) return "--"
        if (seconds < 60) return "${seconds}s"
        val hours = seconds / 3600
        val minutes = (seconds % 3600) / 60
        return if (hours > 0) "${hours}h ${minutes}m" else "${minutes}m"
    }

    fun formatProgress(downloaded: Long, total: Long): String {
        return "${formatSize(downloaded)} / ${formatSize(total)}"
    }

    fun formatPercent(downloaded: Long, total: Long): Float {
        if (total <= 0) return 0f
        return (downloaded.toFloat() / total.toFloat()).coerceIn(0f, 1f)
    }
}
