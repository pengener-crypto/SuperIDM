package com.superidm.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.data.db.DownloadEntity
import com.superidm.data.repository.DownloadRepository
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharingStarted
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.stateIn
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class HistoryViewModel @Inject constructor(
    private val repository: DownloadRepository
) : ViewModel() {

    val completedDownloads: StateFlow<List<DownloadEntity>> = repository.getCompletedDownloads()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    val searchQuery = MutableStateFlow("")

    val filteredDownloads: StateFlow<List<DownloadEntity>> = combine(
        completedDownloads, searchQuery
    ) { downloads, query ->
        if (query.isBlank()) {
            downloads
        } else {
            downloads.filter {
                it.fileName.contains(query, ignoreCase = true) || it.url.contains(query, ignoreCase = true)
            }
        }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), emptyList())

    val totalDataDownloaded: StateFlow<Long> = completedDownloads.map { list ->
        list.sumOf { it.totalBytes }
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), 0L)

    val averageSpeed: StateFlow<Long> = completedDownloads.map { list ->
        if (list.isEmpty()) 0L else list.sumOf { it.averageSpeed } / list.size
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000), 0L)

    fun search(query: String) {
        searchQuery.value = query
    }

    fun deleteDownload(id: String) {
        viewModelScope.launch {
            repository.deleteDownloadById(id)
        }
    }

    fun clearHistory() {
        viewModelScope.launch {
            completedDownloads.value.forEach {
                repository.deleteDownloadById(it.id)
            }
        }
    }
}
