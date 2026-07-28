package com.superidm.data.db

import androidx.room.Database
import androidx.room.RoomDatabase
import androidx.room.TypeConverters
import androidx.room.migration.Migration
import androidx.sqlite.db.SupportSQLiteDatabase
import com.superidm.scheduler.ScheduleDao
import com.superidm.scheduler.ScheduleEntity

@Database(
    entities = [
        DownloadEntity::class,
        SegmentEntity::class,
        PerSiteRuleEntity::class,
        ScheduleEntity::class
    ],
    version = 2,
    exportSchema = false
)
@TypeConverters(Converters::class)
abstract class AppDatabase : RoomDatabase() {
    abstract fun downloadDao(): DownloadDao
    abstract fun segmentDao(): SegmentDao
    abstract fun perSiteRuleDao(): PerSiteRuleDao
    abstract fun scheduleDao(): ScheduleDao
    abstract fun downloadStatsDao(): DownloadStatsDao
    
    companion object {
        val MIGRATION_1_2 = object : Migration(1, 2) {
            override fun migrate(database: SupportSQLiteDatabase) {
                database.execSQL("""
                    CREATE TABLE IF NOT EXISTS per_site_rules (
                        domain TEXT NOT NULL PRIMARY KEY,
                        savePath TEXT NOT NULL DEFAULT '',
                        maxSegments INTEGER NOT NULL DEFAULT 0,
                        preferredQuality TEXT NOT NULL DEFAULT '',
                        proxyHost TEXT NOT NULL DEFAULT '',
                        proxyPort INTEGER NOT NULL DEFAULT 0,
                        userAgent TEXT NOT NULL DEFAULT '',
                        referrer TEXT NOT NULL DEFAULT '',
                        enableCookies INTEGER NOT NULL DEFAULT 1,
                        skipDuplicates INTEGER NOT NULL DEFAULT 1,
                        createdAt INTEGER NOT NULL DEFAULT 0
                    )
                """)
                database.execSQL("""
                    CREATE TABLE IF NOT EXISTS schedules (
                        id TEXT NOT NULL PRIMARY KEY,
                        downloadId TEXT NOT NULL,
                        scheduledStartTime INTEGER NOT NULL,
                        scheduledStopTime INTEGER NOT NULL DEFAULT 0,
                        isRecurring INTEGER NOT NULL DEFAULT 0,
                        recurDaysOfWeek TEXT NOT NULL DEFAULT '',
                        recurHour INTEGER NOT NULL DEFAULT 0,
                        recurMinute INTEGER NOT NULL DEFAULT 0,
                        wifiOnly INTEGER NOT NULL DEFAULT 0,
                        chargingOnly INTEGER NOT NULL DEFAULT 0,
                        targetSsid TEXT NOT NULL DEFAULT '',
                        speedLimitBytesPerSec INTEGER NOT NULL DEFAULT 0,
                        isEnabled INTEGER NOT NULL DEFAULT 1,
                        createdAt INTEGER NOT NULL DEFAULT 0
                    )
                """)
            }
        }
    }
}
