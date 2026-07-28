package com.superidm.scheduler

import android.app.AlarmManager
import android.app.PendingIntent
import android.content.BroadcastReceiver
import android.content.Context
import android.content.Intent
import androidx.work.Constraints
import androidx.work.NetworkType
import androidx.work.WorkManager
import dagger.hilt.android.AndroidEntryPoint
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.GlobalScope
import kotlinx.coroutines.launch
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class SchedulerManager @Inject constructor(
    private val workManager: WorkManager,
    private val scheduleDao: ScheduleDao,
    @ApplicationContext private val context: Context
) {
    suspend fun scheduleDownload(
        downloadId: String,
        startTime: Long,
        wifiOnly: Boolean = false,
        chargingOnly: Boolean = false,
        ssid: String = "",
        speedLimit: Long = 0L
    ) {
        val scheduleId = UUID.randomUUID().toString()
        val entity = ScheduleEntity(
            id = scheduleId,
            downloadId = downloadId,
            scheduledStartTime = startTime,
            wifiOnly = wifiOnly,
            chargingOnly = chargingOnly,
            targetSsid = ssid,
            speedLimitBytesPerSec = speedLimit
        )
        scheduleDao.insert(entity)

        val alarmManager = context.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val intent = Intent(context, ScheduledAlarmReceiver::class.java).apply {
            putExtra("schedule_id", scheduleId)
        }
        val pendingIntent = PendingIntent.getBroadcast(
            context,
            scheduleId.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        alarmManager.setExactAndAllowWhileIdle(AlarmManager.RTC_WAKEUP, startTime, pendingIntent)
    }

    suspend fun cancelSchedule(scheduleId: String) {
        val schedule = scheduleDao.getById(scheduleId) ?: return
        val alarmManager = context.getSystemService(Context.ALARM_SERVICE) as AlarmManager
        val intent = Intent(context, ScheduledAlarmReceiver::class.java).apply {
            putExtra("schedule_id", scheduleId)
        }
        val pendingIntent = PendingIntent.getBroadcast(
            context,
            scheduleId.hashCode(),
            intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE
        )
        alarmManager.cancel(pendingIntent)
        scheduleDao.delete(schedule)
    }

    fun buildWorkConstraints(schedule: ScheduleEntity): Constraints {
        return Constraints.Builder().apply {
            if (schedule.wifiOnly) setRequiredNetworkType(NetworkType.UNMETERED)
            if (schedule.chargingOnly) setRequiresCharging(true)
        }.build()
    }

    suspend fun checkAndStartPending() {
        val pending = scheduleDao.getPending(System.currentTimeMillis())
        for (schedule in pending) {
            val constraints = buildWorkConstraints(schedule)
            val request = DownloadWorker.buildRequest(schedule.downloadId, constraints)
            workManager.enqueue(request)
        }
    }
}

@AndroidEntryPoint
class ScheduledAlarmReceiver : BroadcastReceiver() {
    @Inject lateinit var schedulerManager: SchedulerManager
    @Inject lateinit var scheduleDao: ScheduleDao
    
    override fun onReceive(context: Context, intent: Intent) {
        val scheduleId = intent.getStringExtra("schedule_id") ?: return
        val pendingResult = goAsync()
        GlobalScope.launch(Dispatchers.IO) {
            try {
                val schedule = scheduleDao.getById(scheduleId) ?: return@launch
                if (schedule.isEnabled) {
                    val constraints = schedulerManager.buildWorkConstraints(schedule)
                    val request = DownloadWorker.buildRequest(schedule.downloadId, constraints)
                    WorkManager.getInstance(context).enqueue(request)
                }
            } finally {
                pendingResult.finish()
            }
        }
    }
}
