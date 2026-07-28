package com.superidm.browser

import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.data.db.DownloadEntity
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.engine.QueueManager
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.util.UUID
import javax.inject.Inject

data class BrowserTab(
    val id: String = UUID.randomUUID().toString(),
    val url: String = "",
    val title: String = "New Tab",
    val isPrivate: Boolean = false
)

@HiltViewModel
class BrowserViewModel @Inject constructor(
    private val queueManager: QueueManager,
    private val cookieForwarder: CookieForwarder,
    val videoDetector: VideoDetector,
    private val downloadInterceptor: DownloadInterceptor
) : ViewModel() {

    private val _url = MutableStateFlow("")
    val url = _url.asStateFlow()

    private val _title = MutableStateFlow("New Tab")
    val title = _title.asStateFlow()

    private val _progress = MutableStateFlow(0)
    val progress = _progress.asStateFlow()

    private val _isLoading = MutableStateFlow(false)
    val isLoading = _isLoading.asStateFlow()

    private val _canGoBack = MutableStateFlow(false)
    val canGoBack = _canGoBack.asStateFlow()

    private val _canGoForward = MutableStateFlow(false)
    val canGoForward = _canGoForward.asStateFlow()

    private val _detectedVideos = MutableStateFlow<List<DetectedVideo>>(emptyList())
    val detectedVideos = _detectedVideos.asStateFlow()

    private val _tabs = MutableStateFlow<List<BrowserTab>>(listOf(BrowserTab()))
    val tabs = _tabs.asStateFlow()

    private val _activeTabIndex = MutableStateFlow(0)
    val activeTabIndex = _activeTabIndex.asStateFlow()

    private val _pendingDownload = MutableStateFlow<InterceptedDownload?>(null)
    val pendingDownload = _pendingDownload.asStateFlow()

    fun normalizeUrl(input: String): String {
        return if (!input.startsWith("http://") && !input.startsWith("https://")) {
            "https://$input"
        } else {
            input
        }
    }

    fun loadUrl(inputUrl: String) {
        val normalized = normalizeUrl(inputUrl)
        _url.value = normalized
    }

    fun onPageStarted(url: String) {
        _url.value = url
        _isLoading.value = true
        _progress.value = 0
    }

    fun onPageFinished(url: String, title: String) {
        _isLoading.value = false
        _progress.value = 100
        _title.value = title
        
        val currentTabs = _tabs.value.toMutableList()
        if (_activeTabIndex.value in currentTabs.indices) {
            currentTabs[_activeTabIndex.value] = currentTabs[_activeTabIndex.value].copy(url = url, title = title)
            _tabs.value = currentTabs
        }
    }

    fun onProgressChanged(progress: Int) {
        _progress.value = progress
        if (progress == 100) {
            _isLoading.value = false
        } else {
            _isLoading.value = true
        }
    }

    fun onVideoDetected(url: String, type: String) {
        val current = _detectedVideos.value.toMutableList()
        if (current.none { it.url == url }) {
            current.add(DetectedVideo(url = url, type = type))
            _detectedVideos.value = current
        }
    }

    fun dismissVideoDetection() {
        _detectedVideos.value = emptyList()
    }

    fun setNavigation(canGoBack: Boolean, canGoForward: Boolean) {
        _canGoBack.value = canGoBack
        _canGoForward.value = canGoForward
    }

    fun onDownloadIntercepted(
        url: String, 
        userAgent: String, 
        contentDisposition: String, 
        mimeType: String, 
        contentLength: Long
    ) {
        val cookies = cookieForwarder.getCookiesForUrl(url)
        val download = downloadInterceptor.interceptFromWebView(
            url = url,
            userAgent = userAgent,
            contentDisposition = contentDisposition,
            mimeType = mimeType,
            contentLength = contentLength,
            cookies = cookies,
            referrer = _url.value
        )
        _pendingDownload.value = download
    }

    fun confirmDownload(download: InterceptedDownload, saveDir: String) {
        viewModelScope.launch {
            val entity = DownloadEntity(
                id = UUID.randomUUID().toString(),
                url = download.url,
                fileName = download.suggestedFileName,
                saveDir = saveDir,
                totalBytes = download.contentLength,
                downloadedBytes = 0L,
                status = DownloadStatus.QUEUED,
                priority = DownloadPriority.NORMAL,
                mimeType = download.mimeType,
                userAgent = download.userAgent,
                headers = "{\"Cookie\":\"${download.cookies}\"}",
                referrer = download.referrer
            )
            queueManager.enqueue(entity)
            dismissDownload()
        }
    }

    fun dismissDownload() {
        _pendingDownload.value = null
    }

    fun openNewTab(url: String = "") {
        val newTab = BrowserTab(url = url)
        val currentTabs = _tabs.value.toMutableList()
        currentTabs.add(newTab)
        _tabs.value = currentTabs
        _activeTabIndex.value = currentTabs.size - 1
        if (url.isNotEmpty()) {
            loadUrl(url)
        }
    }

    fun closeTab(id: String) {
        val currentTabs = _tabs.value.toMutableList()
        val indexToRemove = currentTabs.indexOfFirst { it.id == id }
        if (indexToRemove != -1) {
            currentTabs.removeAt(indexToRemove)
            if (currentTabs.isEmpty()) {
                currentTabs.add(BrowserTab())
            }
            _tabs.value = currentTabs
            if (_activeTabIndex.value >= currentTabs.size) {
                _activeTabIndex.value = currentTabs.size - 1
            }
        }
    }

    fun switchTab(index: Int) {
        if (index in _tabs.value.indices) {
            _activeTabIndex.value = index
            val tab = _tabs.value[index]
            _url.value = tab.url
            _title.value = tab.title
        }
    }
}
