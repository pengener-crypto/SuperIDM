package com.superidm.viewmodel

import android.content.Context
import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import javax.inject.Inject

enum class ExportFormat { CSV, JSON }

interface ExportManager {
    suspend fun exportToShareable(format: ExportFormat): Uri?
}

// A dummy implementation for phase 3 compilation
class DummyExportManager @Inject constructor() : ExportManager {
    override suspend fun exportToShareable(format: ExportFormat): Uri? {
        delay(1500)
        return Uri.parse("content://dummy/uri/superidm_history_2026-07-25.${format.name.lowercase()}")
    }
}

@HiltViewModel
class ExportViewModel @Inject constructor(
    private val exportManager: DummyExportManager
) : ViewModel() {

    val format = MutableStateFlow(ExportFormat.CSV)
    val isExporting = MutableStateFlow(false)
    val exportedUri = MutableStateFlow<Uri?>(null)
    val error = MutableStateFlow<String?>(null)

    fun setFormat(newFormat: ExportFormat) {
        format.value = newFormat
        clearExported()
    }

    fun export(context: Context) {
        viewModelScope.launch {
            isExporting.value = true
            error.value = null
            try {
                val uri = withContext(Dispatchers.IO) {
                    exportManager.exportToShareable(format.value)
                }
                if (uri != null) {
                    exportedUri.value = uri
                } else {
                    error.value = "Failed to export history"
                }
            } catch (e: Exception) {
                error.value = e.message ?: "An error occurred during export"
            } finally {
                isExporting.value = false
            }
        }
    }

    fun clearExported() {
        exportedUri.value = null
    }
}
