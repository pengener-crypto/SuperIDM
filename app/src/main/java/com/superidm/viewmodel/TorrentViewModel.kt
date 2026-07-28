package com.superidm.viewmodel

import android.content.Context
import android.net.Uri
import androidx.lifecycle.ViewModel
import androidx.lifecycle.viewModelScope
import com.superidm.protocol.TorrentEngine
import com.superidm.protocol.TorrentFile
import com.superidm.protocol.TorrentStatus
import dagger.hilt.android.lifecycle.HiltViewModel
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch
import java.io.ByteArrayOutputStream
import javax.inject.Inject

@HiltViewModel
class TorrentViewModel @Inject constructor(
    private val engine: TorrentEngine
) : ViewModel() {

    val torrents: StateFlow<Map<String, TorrentStatus>> = engine.activeTorrents

    private val _pendingFiles = MutableStateFlow<List<TorrentFile>?>(null)
    val pendingFiles: StateFlow<List<TorrentFile>?> = _pendingFiles.asStateFlow()

    var pendingTorrentBytes: ByteArray? = null
        private set

    private val _error = MutableStateFlow<String?>(null)
    val error: StateFlow<String?> = _error.asStateFlow()

    init {
        engine.initialize()
        viewModelScope.launch {
            while (true) {
                engine.updateStatuses()
                delay(2000L)
            }
        }
    }

    fun addMagnet(magnetUri: String, savePath: String) {
        if (!magnetUri.startsWith("magnet:")) {
            _error.value = "Invalid magnet URI"
            return
        }
        _error.value = null
        engine.addMagnet(magnetUri, savePath)
    }

    fun loadTorrentFile(uri: Uri, context: Context) {
        try {
            val inputStream = context.contentResolver.openInputStream(uri)
            if (inputStream != null) {
                val buffer = ByteArrayOutputStream()
                var bytesRead: Int
                val data = ByteArray(8192)
                while (inputStream.read(data, 0, data.size).also { bytesRead = it } != -1) {
                    buffer.write(data, 0, bytesRead)
                }
                buffer.flush()
                
                val bytes = buffer.toByteArray()
                pendingTorrentBytes = bytes
                _pendingFiles.value = engine.getFiles(bytes)
                _error.value = null
            } else {
                _error.value = "Could not open torrent file"
            }
        } catch (e: Exception) {
            e.printStackTrace()
            _error.value = "Error parsing torrent file: ${e.message}"
        }
    }

    fun confirmDownload(savePath: String, selectedIndices: IntArray) {
        val bytes = pendingTorrentBytes
        if (bytes != null) {
            // Priority filtering logic would be passed to engine if supported
            // For now, we proceed to download all, ignoring selectedIndices as libtorrent4j priorities require a handle
            engine.addTorrentFile(bytes, savePath)
            pendingTorrentBytes = null
            _pendingFiles.value = null
        }
    }

    fun pause(infoHash: String) {
        engine.pause(infoHash)
    }

    fun resume(infoHash: String) {
        engine.resume(infoHash)
    }

    fun remove(infoHash: String, deleteFiles: Boolean) {
        engine.remove(infoHash, deleteFiles)
    }
    
    fun dismissError() {
        _error.value = null
    }
    
    fun cancelFileSelection() {
        pendingTorrentBytes = null
        _pendingFiles.value = null
    }

    override fun onCleared() {
        super.onCleared()
        engine.shutdown()
    }
}
