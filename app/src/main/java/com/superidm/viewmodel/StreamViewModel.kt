package com.superidm.viewmodel

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.engine.QueueManager
import com.superidm.streaming.StreamDetector
import com.superidm.streaming.StreamInfo
import com.superidm.streaming.StreamQuality
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import javax.inject.Inject

@HiltViewModel
class StreamViewModel @Inject constructor(
    private val streamDetector: StreamDetector,
    private val queueManager: QueueManager
) : ViewModel() {

    private val _streamInfo = MutableStateFlow<StreamInfo?>(null)
    val streamInfo: StateFlow<StreamInfo?> = _streamInfo.asStateFlow()

    private val _isAnalyzing = MutableStateFlow(false)
    val isAnalyzing: StateFlow<Boolean> = _isAnalyzing.asStateFlow()

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    fun analyzeUrl(url: String) {
        viewModelScope.launch {
            _isAnalyzing.value = true
            _error.value = null
            try {
                val info = streamDetector.detect(url)
                if (info != null) {
                    _streamInfo.value = info
                } else {
                    _error.value = "Could not detect stream information."
                }
            } catch (e: Exception) {
                _error.value = e.message ?: "An unknown error occurred"
            } finally {
                _isAnalyzing.value = false
            }
        }
    }

    fun clearError() {
        _error.value = null
    }
}
