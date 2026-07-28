package com.superidm.scheduler

import androidx.room.Entity
import androidx.room.PrimaryKey

@Entity(tableName = "schedules")
data class ScheduleEntity(
    @PrimaryKey val id: String,
    val downloadId: String,
    val scheduledStartTime: Long,
    val scheduledStopTime: Long = 0L,
    val isRecurring: Boolean = false,
    val recurDaysOfWeek: String = "",
    val recurHour: Int = 0,
    val recurMinute: Int = 0,
    val wifiOnly: Boolean = false,
    val chargingOnly: Boolean = false,
    val targetSsid: String = "",
    val speedLimitBytesPerSec: Long = 0L,
    val isEnabled: Boolean = true,
    val createdAt: Long = System.currentTimeMillis()
)
