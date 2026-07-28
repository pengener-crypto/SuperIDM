package com.superidm.scheduler

import androidx.room.*
import kotlinx.coroutines.flow.Flow

@Dao
interface ScheduleDao {
    @Insert(onConflict = OnConflictStrategy.REPLACE)
    suspend fun insert(entity: ScheduleEntity)
    
    @Update
    suspend fun update(entity: ScheduleEntity)
    
    @Delete
    suspend fun delete(entity: ScheduleEntity)
    
    @Query("SELECT * FROM schedules ORDER BY scheduledStartTime ASC")
    fun getAll(): Flow<List<ScheduleEntity>>
    
    @Query("SELECT * FROM schedules WHERE id = :id")
    suspend fun getById(id: String): ScheduleEntity?
    
    @Query("SELECT * FROM schedules WHERE downloadId = :downloadId LIMIT 1")
    suspend fun getByDownloadId(downloadId: String): ScheduleEntity?
    
    @Query("SELECT * FROM schedules WHERE scheduledStartTime <= :currentTime AND isEnabled = 1")
    suspend fun getPending(currentTime: Long): List<ScheduleEntity>
    
    @Query("DELETE FROM schedules WHERE id = :id")
    suspend fun deleteById(id: String)
    
    @Query("DELETE FROM schedules WHERE downloadId = :downloadId")
    suspend fun deleteByDownloadId(downloadId: String)
}
