package com.superidm.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.data.db.CategoryCount
import com.superidm.data.db.DailyStats
import com.superidm.data.db.DownloadEntity
import com.superidm.data.db.DownloadStatsDao
import com.superidm.data.db.HourlyStats
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.*
import javax.inject.Inject

@HiltViewModel
class StatisticsViewModel @Inject constructor(
    private val statsDao: DownloadStatsDao
) : ViewModel() {

    val totalDownloadedBytes: StateFlow<Long> = statsDao.getTotalDownloadedBytes()
        .map { it ?: 0L }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0L)

    val averageSpeed: StateFlow<Long> = statsDao.getAverageSpeed()
        .map { it ?: 0L }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0L)

    val totalCount: StateFlow<Int> = statsDao.getTotalDownloadCount()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0)

    val completedCount: StateFlow<Int> = statsDao.getCompletedCount()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0)

    val failedCount: StateFlow<Int> = statsDao.getFailedCount()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0)

    val categoryBreakdown: StateFlow<List<CategoryCount>> = statsDao.getCountByCategory()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), emptyList())

    val dailyStats: StateFlow<List<DailyStats>> = statsDao.getDailyDownloadBytes(30)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), emptyList())

    val topDownloads: StateFlow<List<DownloadEntity>> = statsDao.getTopDownloadsBySpeed(10)
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), emptyList())

    val todayBytes: StateFlow<Long> = statsDao.getTotalDownloadedToday()
        .map { it ?: 0L }
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0L)

    val hourlyActivity: StateFlow<List<HourlyStats>> = statsDao.getHourlyActivity()
        .stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), emptyList())

    val successRate: StateFlow<Float> = combine(completedCount, totalCount) { completed, total ->
        if (total > 0) (completed.toFloat() / total.toFloat()) * 100f else 0f
    }.stateIn(viewModelScope, SharingStarted.WhileSubscribed(5000L), 0f)
}
