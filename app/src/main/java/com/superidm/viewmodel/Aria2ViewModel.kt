package com.superidm.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.aria2.Aria2DownloadStatus
import com.superidm.aria2.Aria2GlobalStat
import com.superidm.aria2.Aria2Manager
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class Aria2ViewModel @Inject constructor(
    private val aria2Manager: Aria2Manager
) : ViewModel() {

    val isConnected: StateFlow<Boolean> = aria2Manager.isConnected.asStateFlow()
    val downloads: StateFlow<List<Aria2DownloadStatus>> = aria2Manager.allDownloads.asStateFlow()
    val globalStat: StateFlow<Aria2GlobalStat?> = aria2Manager.globalStat.asStateFlow()

    val activeDownloads = downloads.map { list -> list.filter { it.status == "active" } }.stateIn(viewModelScope, SharingStarted.Lazily, emptyList())
    val waitingDownloads = downloads.map { list -> list.filter { it.status == "waiting" || it.status == "paused" } }.stateIn(viewModelScope, SharingStarted.Lazily, emptyList())
    val completedDownloads = downloads.map { list -> list.filter { it.status == "complete" || it.status == "error" || it.status == "removed" } }.stateIn(viewModelScope, SharingStarted.Lazily, emptyList())

    val urlToAdd = MutableStateFlow("")
    val error = MutableStateFlow<String?>(null)
    val isTestingConnection = MutableStateFlow(false)
    
    val downloadSpeedLimit = MutableStateFlow(0f)
    val uploadSpeedLimit = MutableStateFlow(0f)

    init {
        viewModelScope.launch {
            aria2Manager.loadConfig()
            startPolling()
        }
    }

    override fun onCleared() {
        super.onCleared()
        stopPolling()
    }

    fun startPolling() {
        aria2Manager.startPolling()
    }

    fun stopPolling() {
        aria2Manager.stopPolling()
    }

    fun testConnection() {
        viewModelScope.launch {
            isTestingConnection.value = true
            val success = aria2Manager.testConnection()
            if (!success) {
                error.value = "Connection failed. Please check your settings."
            }
            isTestingConnection.value = false
        }
    }

    fun addUrl(url: String, savePath: String) {
        viewModelScope.launch {
            if (url.isBlank()) {
                error.value = "URL cannot be empty"
                return@launch
            }
            val gid = aria2Manager.addUri(url, savePath)
            if (gid == null) {
                error.value = "Failed to add download"
            } else {
                urlToAdd.value = ""
            }
        }
    }

    fun pause(gid: String) {
        viewModelScope.launch { aria2Manager.pause(gid) }
    }

    fun resume(gid: String) {
        viewModelScope.launch { aria2Manager.resume(gid) }
    }

    fun remove(gid: String, deleteFiles: Boolean) {
        viewModelScope.launch { 
            // aria2 doesn't have a specific option for deleting files in the remove command itself via basic JSON-RPC.
            // But we can remove it.
            aria2Manager.remove(gid) 
        }
    }

    fun pauseAll() {
        viewModelScope.launch { aria2Manager.pauseAll() }
    }

    fun resumeAll() {
        viewModelScope.launch { aria2Manager.resumeAll() }
    }

    fun clearCompleted() {
        viewModelScope.launch { aria2Manager.purgeCompleted() }
    }

    fun setSpeedLimit(downloadMbps: Float, uploadMbps: Float) {
        viewModelScope.launch {
            downloadSpeedLimit.value = downloadMbps
            uploadSpeedLimit.value = uploadMbps
            aria2Manager.setSpeedLimit(downloadMbps, uploadMbps)
        }
    }

    fun saveConfig(url: String, secret: String) {
        viewModelScope.launch {
            aria2Manager.saveConfig(url, secret)
        }
    }
}
