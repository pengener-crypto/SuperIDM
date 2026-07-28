package com.superidm.service

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import androidx.core.app.ServiceCompat
import com.superidm.engine.DownloadEngine
import com.superidm.engine.QueueManager
import com.superidm.notifications.NotificationHelper
import dagger.hilt.android.AndroidEntryPoint
import kotlinx.coroutines.*
import javax.inject.Inject

@AndroidEntryPoint
class DownloadService : Service() {

    @Inject lateinit var downloadEngine: DownloadEngine
    @Inject lateinit var queueManager: QueueManager
    @Inject lateinit var notificationHelper: NotificationHelper

    private val serviceScope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private var notificationJob: Job? = null
    private var activeCount = 0

    companion object {
        const val ACTION_START = "ACTION_START"
        const val ACTION_PAUSE = "ACTION_PAUSE"
        const val ACTION_RESUME = "ACTION_RESUME"
        const val ACTION_CANCEL = "ACTION_CANCEL"
        const val ACTION_PAUSE_ALL = "ACTION_PAUSE_ALL"
        const val ACTION_RESUME_ALL = "ACTION_RESUME_ALL"
        const val EXTRA_DOWNLOAD_ID = "EXTRA_DOWNLOAD_ID"

        fun startService(context: Context, action: String, downloadId: String? = null): Intent {
            return Intent(context, DownloadService::class.java).apply {
                this.action = action
                if (downloadId != null) putExtra(EXTRA_DOWNLOAD_ID, downloadId)
            }
        }
    }

    override fun onCreate() {
        super.onCreate()
        notificationHelper.createChannels()
        startForegroundService()
        observeProgress()
    }

    private fun startForegroundService() {
        val notification = notificationHelper.buildActiveDownloadNotification(0, 0L, 0L)
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            ServiceCompat.startForeground(this, 1, notification, ServiceInfo.FOREGROUND_SERVICE_TYPE_DATA_SYNC)
        } else {
            startForeground(1, notification)
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val action = intent?.action
        val downloadId = intent?.getStringExtra(EXTRA_DOWNLOAD_ID)

        when (action) {
            ACTION_START -> if (downloadId != null) downloadEngine.startDownload(downloadId)
            ACTION_PAUSE -> if (downloadId != null) downloadEngine.pauseDownload(downloadId)
            ACTION_RESUME -> if (downloadId != null) downloadEngine.resumeDownload(downloadId)
            ACTION_CANCEL -> if (downloadId != null) downloadEngine.cancelDownload(downloadId)
            ACTION_PAUSE_ALL -> queueManager.pauseAll()
            ACTION_RESUME_ALL -> queueManager.resumeAll()
        }

        return START_STICKY
    }

    private fun observeProgress() {
        notificationJob = serviceScope.launch {
            var lastUpdate = System.currentTimeMillis()
            var speedAcc = 0L
            downloadEngine.progressFlow.collect { progress ->
                val now = System.currentTimeMillis()
                speedAcc += progress.speed
                
                if (now - lastUpdate >= 2000) {
                    val notif = notificationHelper.buildActiveDownloadNotification(1, progress.speed, progress.eta)
                    val manager = getSystemService(Context.NOTIFICATION_SERVICE) as android.app.NotificationManager
                    manager.notify(1, notif)
                    lastUpdate = now
                    speedAcc = 0L
                }
            }
        }
    }

    override fun onDestroy() {
        super.onDestroy()
        serviceScope.cancel()
    }

    override fun onBind(intent: Intent?): IBinder? = null
}
