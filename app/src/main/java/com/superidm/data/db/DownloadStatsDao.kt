package com.superidm.data.db

import androidx.room.Dao
import androidx.room.Query
import kotlinx.coroutines.flow.Flow

data class CategoryCount(val categoryFolder: String, val count: Int)

data class DailyStats(val dayTimestamp: Long, val totalBytes: Long, val count: Int)

data class HourlyStats(val hour: Int, val count: Int)

@Dao
interface DownloadStatsDao {

    @Query("SELECT SUM(downloadedBytes) FROM downloads WHERE status = 'COMPLETED'")
    fun getTotalDownloadedBytes(): Flow<Long?>

    @Query("SELECT AVG(averageSpeed) FROM downloads WHERE status = 'COMPLETED' AND averageSpeed > 0")
    fun getAverageSpeed(): Flow<Long?>

    @Query("SELECT COUNT(*) FROM downloads")
    fun getTotalDownloadCount(): Flow<Int>

    @Query("SELECT COUNT(*) FROM downloads WHERE status = 'COMPLETED'")
    fun getCompletedCount(): Flow<Int>

    @Query("SELECT COUNT(*) FROM downloads WHERE status = 'FAILED'")
    fun getFailedCount(): Flow<Int>

    @Query("SELECT categoryFolder, COUNT(*) as count FROM downloads GROUP BY categoryFolder")
    fun getCountByCategory(): Flow<List<CategoryCount>>

    @Query("""
        SELECT 
            CAST(strftime('%s', strftime('%Y-%m-%d', completedAt/1000, 'unixepoch')) AS INTEGER) * 1000 AS dayTimestamp,
            SUM(downloadedBytes) AS totalBytes,
            COUNT(id) AS count
        FROM downloads
        WHERE status = 'COMPLETED' AND completedAt >= (strftime('%s', 'now', '-' || :daysBack || ' days') * 1000)
        GROUP BY strftime('%Y-%m-%d', completedAt/1000, 'unixepoch')
        ORDER BY dayTimestamp ASC
    """)
    fun getDailyDownloadBytes(daysBack: Int): Flow<List<DailyStats>>

    @Query("SELECT * FROM downloads ORDER BY averageSpeed DESC LIMIT :limit")
    fun getTopDownloadsBySpeed(limit: Int = 10): Flow<List<DownloadEntity>>

    @Query("""
        SELECT SUM(downloadedBytes) 
        FROM downloads 
        WHERE status = 'COMPLETED' AND completedAt >= (CAST(strftime('%s', 'now', 'start of day') AS INTEGER) * 1000)
    """)
    fun getTotalDownloadedToday(): Flow<Long?>

    @Query("""
        SELECT 
            CAST(strftime('%H', completedAt/1000, 'unixepoch') AS INTEGER) AS hour, 
            COUNT(id) AS count
        FROM downloads 
        WHERE status = 'COMPLETED'
        GROUP BY hour
        ORDER BY hour ASC
    """)
    fun getHourlyActivity(): Flow<List<HourlyStats>>
}
