package com.superidm.widget

import android.content.Context
import androidx.glance.appwidget.updateAll
import androidx.hilt.work.HiltWorker
import androidx.work.*
import com.superidm.data.repository.DownloadRepository
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject
import kotlinx.coroutines.flow.first
import java.util.concurrent.TimeUnit

@HiltWorker
class WidgetUpdateWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted params: WorkerParameters,
    private val repository: DownloadRepository
) : CoroutineWorker(context, params) {
    
    companion object {
        private const val UNIQUE_WORK_NAME = "superidm_widget_update"
        
        fun schedulePeriodicUpdate(context: Context) {
            val request = PeriodicWorkRequestBuilder<WidgetUpdateWorker>(15, TimeUnit.MINUTES)
                .build()
            WorkManager.getInstance(context).enqueueUniquePeriodicWork(
                UNIQUE_WORK_NAME,
                ExistingPeriodicWorkPolicy.KEEP,
                request
            )
        }
    }
    
    override suspend fun doWork(): Result {
        // Get active downloads
        val activeDownloadsFlow = repository.getActiveDownloads()
        val activeDownloads = activeDownloadsFlow.first()
        
        // Create WidgetState from active downloads
        val widgetDownloads = activeDownloads.take(3).map { entity ->
            WidgetDownload(
                name = entity.fileName,
                progress = if (entity.totalBytes > 0) entity.downloadedBytes.toFloat() / entity.totalBytes else 0f,
                speed = entity.averageSpeed ?: 0L
            )
        }
        val totalSpeed = activeDownloads.sumOf { it.averageSpeed ?: 0L }
        
        val widgetState = WidgetState(
            activeDownloads = widgetDownloads,
            totalSpeed = totalSpeed
        )
        
        // Update the widget
        // Note: GlanceStateDefinition needed for state update in a fully implemented system.
        // For simplicity, just trigger a full widget update
        SuperIDMWidget().updateAll(applicationContext)
        return Result.success()
    }
}
