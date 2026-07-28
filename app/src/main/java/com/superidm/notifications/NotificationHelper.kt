package com.superidm.notifications

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.os.Build
import androidx.core.app.NotificationCompat
import com.superidm.data.db.DownloadEntity
import com.superidm.service.DownloadService
import com.superidm.util.FormatUtils
import dagger.hilt.android.qualifiers.ApplicationContext
import javax.inject.Inject

class NotificationHelper @Inject constructor(@ApplicationContext private val context: Context) {

    companion object {
        const val CHANNEL_DOWNLOAD = "superidm_download"
        const val CHANNEL_COMPLETE = "superidm_complete"
    }

    private val notificationManager = context.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager

    fun createChannels() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val downloadChannel = NotificationChannel(
                CHANNEL_DOWNLOAD,
                "Active Downloads",
                NotificationManager.IMPORTANCE_LOW
            )
            val completeChannel = NotificationChannel(
                CHANNEL_COMPLETE,
                "Completed Downloads",
                NotificationManager.IMPORTANCE_HIGH
            )
            notificationManager.createNotificationChannel(downloadChannel)
            notificationManager.createNotificationChannel(completeChannel)
        }
    }

    fun buildActiveDownloadNotification(activeCount: Int, speed: Long, eta: Long): Notification {
        val pauseAllIntent = PendingIntent.getService(
            context, 0, DownloadService.startService(context, DownloadService.ACTION_PAUSE_ALL),
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )
        
        return NotificationCompat.Builder(context, CHANNEL_DOWNLOAD)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(if (activeCount > 0) "Downloading $activeCount files..." else "SuperIDM Active")
            .setContentText("Speed: ${FormatUtils.formatSpeed(speed)} - ETA: ${FormatUtils.formatEta(eta)}")
            .setOngoing(true)
            .addAction(android.R.drawable.ic_media_pause, "Pause All", pauseAllIntent)
            .build()
    }

    fun buildSingleDownloadNotification(entity: DownloadEntity, progressPercent: Int, speed: Long): Notification {
        return NotificationCompat.Builder(context, CHANNEL_DOWNLOAD)
            .setSmallIcon(android.R.drawable.stat_sys_download)
            .setContentTitle(entity.fileName)
            .setContentText(FormatUtils.formatSpeed(speed))
            .setProgress(100, progressPercent, false)
            .build()
    }

    fun showCompletionNotification(entity: DownloadEntity) {
        val notification = NotificationCompat.Builder(context, CHANNEL_COMPLETE)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle("Download Complete")
            .setContentText(entity.fileName)
            .setAutoCancel(true)
            .build()
        notificationManager.notify(entity.id.hashCode(), notification)
    }

    fun showBatchCompletionNotification(count: Int) {
        val notification = NotificationCompat.Builder(context, CHANNEL_COMPLETE)
            .setSmallIcon(android.R.drawable.stat_sys_download_done)
            .setContentTitle("Downloads Complete")
            .setContentText("$count files downloaded successfully.")
            .setAutoCancel(true)
            .build()
        notificationManager.notify("batch".hashCode(), notification)
    }

    fun dismiss(notificationId: Int) {
        notificationManager.cancel(notificationId)
    }

    fun dismissAll() {
        notificationManager.cancelAll()
    }
}
