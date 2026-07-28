package com.superidm.viewmodel

import android.content.Context
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.data.db.DownloadEntity
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.data.repository.DownloadRepository
import com.superidm.engine.DownloadEngine
import com.superidm.engine.DownloadProgress
import com.superidm.engine.QueueManager
import com.superidm.service.DownloadService
import dagger.hilt.android.lifecycle.HiltViewModel
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.scan
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import java.util.UUID
import javax.inject.Inject

@HiltViewModel
class HomeViewModel @Inject constructor(
    private val queueManager: QueueManager,
    private val downloadEngine: DownloadEngine,
    private val repository: DownloadRepository,
    @ApplicationContext private val context: Context
) : ViewModel() {

    val activeDownloads: StateFlow<List<DownloadEntity>> = repository.getActiveDownloads()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    val downloadProgress: StateFlow<Map<String, DownloadProgress>> = downloadEngine.progressFlow
        .scan(emptyMap<String, DownloadProgress>()) { acc, progress ->
            acc + (progress.downloadId to progress)
        }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyMap())

    val activeCount: StateFlow<Int> = activeDownloads.map { list ->
        list.count { it.status == DownloadStatus.DOWNLOADING || it.status == DownloadStatus.CONNECTING }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), 0)

    val totalSpeed: StateFlow<Long> = downloadProgress.map { map ->
        map.values.sumOf { it.speed }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), 0L)

    fun addDownload(
        url: String,
        fileName: String,
        saveDir: String,
        priority: DownloadPriority,
        headers: Map<String, String> = emptyMap(),
        referrer: String = ""
    ) {
        viewModelScope.launch {
            val id = UUID.randomUUID().toString()
            val entity = DownloadEntity(
                id = id,
                url = url,
                fileName = fileName,
                saveDir = saveDir,
                totalBytes = 0L,
                downloadedBytes = 0L,
                status = DownloadStatus.QUEUED,
                priority = priority,
                mimeType = "",
                createdAt = System.currentTimeMillis(),
                completedAt = 0L,
                averageSpeed = 0L,
                categoryFolder = "",
                errorMessage = ""
            )
            repository.insertDownload(entity)
            queueManager.enqueue(entity)
            context.startService(DownloadService.startService(context, DownloadService.ACTION_START, id))
        }
    }

    fun pauseDownload(id: String) {
        viewModelScope.launch {
            context.startService(DownloadService.startService(context, DownloadService.ACTION_PAUSE, id))
        }
    }

    fun resumeDownload(id: String) {
        viewModelScope.launch {
            context.startService(DownloadService.startService(context, DownloadService.ACTION_RESUME, id))
        }
    }

    fun cancelDownload(id: String) {
        viewModelScope.launch {
            context.startService(DownloadService.startService(context, DownloadService.ACTION_CANCEL, id))
        }
    }

    fun pauseAll() {
        viewModelScope.launch {
            queueManager.pauseAll()
            context.startService(DownloadService.startService(context, DownloadService.ACTION_PAUSE_ALL))
        }
    }

    fun resumeAll() {
        viewModelScope.launch {
            queueManager.resumeAll()
            context.startService(DownloadService.startService(context, DownloadService.ACTION_RESUME_ALL))
        }
    }

    fun reorderPriority(id: String, priority: DownloadPriority) {
        viewModelScope.launch {
            queueManager.reorderPriority(id, priority)
        }
    }
}
