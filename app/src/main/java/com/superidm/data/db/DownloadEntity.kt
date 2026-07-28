package com.superidm.data.db

import androidx.room.Entity
import androidx.room.PrimaryKey
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus

@Entity(tableName = "downloads")
data class DownloadEntity(
    @PrimaryKey val id: String,
    val url: String,
    val fileName: String,
    val saveDir: String,
    val totalBytes: Long = -1L,
    val downloadedBytes: Long = 0L,
    val status: DownloadStatus = DownloadStatus.QUEUED,
    val priority: DownloadPriority = DownloadPriority.NORMAL,
    val segmentCount: Int = 8,
    val mimeType: String = "",
    val referrer: String = "",
    val userAgent: String = "SuperIDM/1.0 (Android)",
    val headers: String = "{}",
    val createdAt: Long = System.currentTimeMillis(),
    val completedAt: Long = 0L,
    val averageSpeed: Long = 0L,
    val categoryFolder: String = "",
    val checksum: String = "",
    val resumable: Boolean = false,
    val errorMessage: String = ""
)
