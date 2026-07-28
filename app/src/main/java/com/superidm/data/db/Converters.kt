package com.superidm.data.db

import androidx.room.TypeConverter
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.data.model.SegmentStatus

class Converters {
    @TypeConverter fun fromDownloadStatus(value: DownloadStatus): String = value.name
    @TypeConverter fun toDownloadStatus(value: String): DownloadStatus = DownloadStatus.valueOf(value)
    @TypeConverter fun fromDownloadPriority(value: DownloadPriority): String = value.name
    @TypeConverter fun toDownloadPriority(value: String): DownloadPriority = DownloadPriority.valueOf(value)
    @TypeConverter fun fromSegmentStatus(value: SegmentStatus): String = value.name
    @TypeConverter fun toSegmentStatus(value: String): SegmentStatus = SegmentStatus.valueOf(value)
}
