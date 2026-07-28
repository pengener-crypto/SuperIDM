package com.superidm.data.db

import androidx.room.Entity
import androidx.room.ForeignKey
import androidx.room.Index
import androidx.room.PrimaryKey
import com.superidm.data.model.SegmentStatus

@Entity(
    tableName = "segments",
    foreignKeys = [
        ForeignKey(
            entity = DownloadEntity::class,
            parentColumns = ["id"],
            childColumns = ["downloadId"],
            onDelete = ForeignKey.CASCADE
        )
    ],
    indices = [Index("downloadId")]
)
data class SegmentEntity(
    @PrimaryKey val id: String,
    val downloadId: String,
    val index: Int,
    val startByte: Long,
    val endByte: Long,
    val downloadedBytes: Long = 0L,
    val status: SegmentStatus = SegmentStatus.PENDING,
    val tempFilePath: String = "",
    val retryCount: Int = 0
)
