package com.superidm.data.db

import androidx.room.Dao
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import com.superidm.data.model.SegmentStatus

@Dao
interface SegmentDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insertAll(segments: List<SegmentEntity>)

    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(segment: SegmentEntity)

    @Query("SELECT * FROM segments WHERE downloadId = :downloadId ORDER BY `index` ASC")
    suspend fun getByDownloadId(downloadId: String): List<SegmentEntity>

    @Query("UPDATE segments SET downloadedBytes = :downloadedBytes, status = :status WHERE id = :id")
    suspend fun updateProgress(id: String, downloadedBytes: Long, status: SegmentStatus)

    @Query("UPDATE segments SET status = :status WHERE id = :id")
    suspend fun updateStatus(id: String, status: SegmentStatus)

    @Query("DELETE FROM segments WHERE downloadId = :downloadId")
    suspend fun deleteByDownloadId(downloadId: String)

    @Query("SELECT COUNT(*) FROM segments WHERE downloadId = :downloadId AND status = 'COMPLETED'")
    suspend fun getCompletedCount(downloadId: String): Int

    @Query("UPDATE segments SET retryCount = retryCount + 1 WHERE id = :id")
    suspend fun incrementRetry(id: String)
}
