package com.superidm.data.model

enum class DownloadPriority(val label: String, val order: Int) {
    CRITICAL("Critical", 0),
    HIGH("High", 1),
    NORMAL("Normal", 2),
    LOW("Low", 3)
}
