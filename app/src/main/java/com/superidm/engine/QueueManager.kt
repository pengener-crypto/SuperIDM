package com.superidm.engine

import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.intPreferencesKey
import com.superidm.data.db.DownloadEntity
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.data.repository.DownloadRepository
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch
import javax.inject.Inject

class QueueManager @Inject constructor(
    private val repository: DownloadRepository,
    private val downloadEngine: DownloadEngine,
    private val dataStore: DataStore<Preferences>
) {

    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val CONCURRENT_LIMIT_KEY = intPreferencesKey("max_concurrent_downloads")

    fun startQueue() {
        scope.launch {
            processQueue()
        }
    }

    fun enqueue(entity: DownloadEntity) {
        scope.launch {
            repository.insertDownload(entity)
            processQueue()
        }
    }

    fun pauseAll() {
        scope.launch {
            val active: List<DownloadEntity> = repository.getDownloadsByStatus(listOf(DownloadStatus.DOWNLOADING, DownloadStatus.CONNECTING))
            for (download in active) {
                downloadEngine.pauseDownload(download.id)
            }
        }
    }

    fun resumeAll() {
        scope.launch {
            val paused: List<DownloadEntity> = repository.getDownloadsByStatus(listOf(DownloadStatus.PAUSED))
            for (download in paused) {
                repository.updateStatus(download.id, DownloadStatus.QUEUED)
            }
            processQueue()
        }
    }

    fun setMaxConcurrent(count: Int) {
        scope.launch {
            dataStore.edit { prefs ->
                prefs[CONCURRENT_LIMIT_KEY] = count
            }
            processQueue()
        }
    }

    private suspend fun processQueue() {
        val limit = dataStore.data.first()[CONCURRENT_LIMIT_KEY] ?: 3
        val activeDownloads: List<DownloadEntity> = repository.getDownloadsByStatus(listOf(DownloadStatus.DOWNLOADING, DownloadStatus.CONNECTING))
        
        if (activeDownloads.size < limit) {
            val queuedDownloads: List<DownloadEntity> = repository.getDownloadsByStatus(listOf(DownloadStatus.QUEUED))
                .sortedByDescending { it.priority.ordinal }
                
            val toStart = limit - activeDownloads.size
            for (i in 0 until minOf(toStart, queuedDownloads.size)) {
                downloadEngine.startDownload(queuedDownloads[i].id)
            }
        }
    }

    fun reorderPriority(downloadId: String, priority: DownloadPriority) {
        scope.launch {
            repository.updatePriority(downloadId, priority)
            processQueue()
        }
    }
}
