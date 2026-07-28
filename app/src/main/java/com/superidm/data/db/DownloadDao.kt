package com.superidm.data.db

import androidx.room.Dao
import androidx.room.Delete
import androidx.room.Insert
import androidx.room.OnConflictStrategy
import androidx.room.Query
import androidx.room.Update
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import kotlinx.coroutines.flow.Flow

@Dao
interface DownloadDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entity: DownloadEntity)

    @Update
    suspend fun update(entity: DownloadEntity)

    @Delete
    suspend fun delete(entity: DownloadEntity)

    @Query("DELETE FROM downloads WHERE id = :id")
    suspend fun deleteById(id: String)

    @Query("SELECT * FROM downloads ORDER BY CASE priority WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 WHEN 'NORMAL' THEN 2 WHEN 'LOW' THEN 3 ELSE 4 END ASC, createdAt DESC")
    fun getAll(): Flow<List<DownloadEntity>>

    @Query("SELECT * FROM downloads WHERE status IN ('QUEUED', 'CONNECTING', 'DOWNLOADING', 'PAUSED') ORDER BY CASE priority WHEN 'CRITICAL' THEN 0 WHEN 'HIGH' THEN 1 WHEN 'NORMAL' THEN 2 WHEN 'LOW' THEN 3 ELSE 4 END ASC, createdAt DESC")
    fun getActive(): Flow<List<DownloadEntity>>

    @Query("SELECT * FROM downloads WHERE status = 'COMPLETED' ORDER BY createdAt DESC")
    fun getCompleted(): Flow<List<DownloadEntity>>

    @Query("SELECT * FROM downloads WHERE id = :id")
    suspend fun getById(id: String): DownloadEntity?

    @Query("SELECT * FROM downloads WHERE status IN (:statuses)")
    suspend fun getByStatuses(statuses: List<DownloadStatus>): List<DownloadEntity>

    @Query("UPDATE downloads SET status = :status WHERE id = :id")
    suspend fun updateStatus(id: String, status: DownloadStatus)

    @Query("UPDATE downloads SET priority = :priority WHERE id = :id")
    suspend fun updatePriority(id: String, priority: DownloadPriority)

    @Query("UPDATE downloads SET downloadedBytes = :downloadedBytes, averageSpeed = :speed WHERE id = :id")
    suspend fun updateProgress(id: String, downloadedBytes: Long, speed: Long)

    @Query("UPDATE downloads SET status = 'COMPLETED', completedAt = :completedAt, averageSpeed = :averageSpeed WHERE id = :id")
    suspend fun updateCompletion(id: String, completedAt: Long, averageSpeed: Long)

    @Query("UPDATE downloads SET resumable = :resumable, totalBytes = :totalBytes WHERE id = :id")
    suspend fun updateResumable(id: String, resumable: Boolean, totalBytes: Long)

    @Query("UPDATE downloads SET errorMessage = :errorMessage WHERE id = :id")
    suspend fun updateErrorMessage(id: String, errorMessage: String)

    @Query("UPDATE downloads SET totalBytes = :totalBytes, resumable = :resumable, mimeType = :mimeType, fileName = :fileName WHERE id = :id")
    suspend fun updateDownloadInfo(id: String, totalBytes: Long, resumable: Boolean, mimeType: String, fileName: String)

    @Query("UPDATE downloads SET completedAt = :completedAt WHERE id = :id")
    suspend fun updateCompletedTime(id: String, completedAt: Long)

    @Query("UPDATE downloads SET downloadedBytes = :downloadedBytes WHERE id = :id")
    suspend fun updateDownloadedBytes(id: String, downloadedBytes: Long)

    @Query("SELECT * FROM downloads WHERE fileName LIKE '%' || :query || '%' OR url LIKE '%' || :query || '%' ORDER BY createdAt DESC")
    fun search(query: String): Flow<List<DownloadEntity>>

    @Query("SELECT * FROM downloads WHERE categoryFolder = :category ORDER BY createdAt DESC")
    fun getByCategory(category: String): Flow<List<DownloadEntity>>

    @Query("SELECT COUNT(*) FROM downloads WHERE status = :status")
    suspend fun countByStatus(status: DownloadStatus): Int
}
