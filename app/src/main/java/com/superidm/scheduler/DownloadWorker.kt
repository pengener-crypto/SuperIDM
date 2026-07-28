package com.superidm.scheduler

import android.content.Context
import android.content.Intent
import androidx.core.content.ContextCompat
import androidx.hilt.work.HiltWorker
import androidx.work.*
import com.superidm.data.repository.DownloadRepository
import com.superidm.service.DownloadService
import dagger.assisted.Assisted
import dagger.assisted.AssistedInject

@HiltWorker
class DownloadWorker @AssistedInject constructor(
    @Assisted context: Context,
    @Assisted params: WorkerParameters,
    private val repository: DownloadRepository
) : CoroutineWorker(context, params) {
    companion object {
        const val KEY_DOWNLOAD_ID = "download_id"
        
        fun buildRequest(downloadId: String, constraints: Constraints = Constraints.NONE): OneTimeWorkRequest {
            return OneTimeWorkRequestBuilder<DownloadWorker>()
                .setInputData(workDataOf(KEY_DOWNLOAD_ID to downloadId))
                .setConstraints(constraints)
                .build()
        }
    }
    
    override suspend fun doWork(): Result {
        val downloadId = inputData.getString(KEY_DOWNLOAD_ID) ?: return Result.failure()
        // Start the download service with this downloadId
        val intent = Intent(applicationContext, DownloadService::class.java).apply {
            action = "ACTION_START"
            putExtra("download_id", downloadId)
        }
        ContextCompat.startForegroundService(applicationContext, intent)
        return Result.success()
    }
}
