package com.superidm.protocol

import android.content.Context
import kotlinx.coroutines.flow.MutableStateFlow
import org.libtorrent4j.AlertListener
import org.libtorrent4j.SessionHandle
import org.libtorrent4j.SessionManager
import org.libtorrent4j.SettingsPack
import org.libtorrent4j.TorrentHandle
import org.libtorrent4j.TorrentInfo
import org.libtorrent4j.AddTorrentParams
import org.libtorrent4j.Sha1Hash
import org.libtorrent4j.alerts.*
import java.io.File

data class TorrentStatus(
    val name: String,
    val progress: Float,
    val downloadRate: Long,
    val uploadRate: Long,
    val seeders: Int,
    val peers: Int,
    val totalDone: Long,
    val totalSize: Long,
    val state: String,
    val infoHash: String
)

data class TorrentFile(
    val name: String,
    val size: Long,
    val progress: Float,
    val priority: Int
)

class TorrentEngine(private val context: Context) {
    val activeTorrents = MutableStateFlow<Map<String, TorrentStatus>>(emptyMap())
    private var session: SessionManager? = null

    fun initialize() {
        session = SessionManager()
        session!!.start()
    }

    fun addMagnet(magnetUri: String, savePath: String) {
        val saveDir = File(savePath)
        if (!saveDir.exists()) saveDir.mkdirs()
        session?.download(magnetUri, saveDir, org.libtorrent4j.swig.torrent_flags_t())
    }

    fun addTorrentFile(torrentBytes: ByteArray, savePath: String) {
        val saveDir = File(savePath)
        if (!saveDir.exists()) saveDir.mkdirs()
        val torrentInfo = TorrentInfo(torrentBytes)
        session?.download(torrentInfo, saveDir)
    }

    fun pause(infoHash: String) {
        val handle = findHandle(infoHash)
        handle?.pause()
    }

    fun resume(infoHash: String) {
        val handle = findHandle(infoHash)
        handle?.resume()
    }

    fun remove(infoHash: String, deleteFiles: Boolean = false) {
        val handle = findHandle(infoHash)
        handle?.let {
            if (deleteFiles) {
                session?.remove(it, org.libtorrent4j.swig.session_handle.delete_files)
            } else {
                session?.remove(it)
            }
        }
    }

    fun getFiles(torrentBytes: ByteArray): List<TorrentFile> {
        val torrentInfo = TorrentInfo(torrentBytes)
        val files = mutableListOf<TorrentFile>()
        val storage = torrentInfo.files()
        
        for (i in 0 until storage.numFiles()) {
            files.add(
                TorrentFile(
                    name = storage.filePath(i),
                    size = storage.fileSize(i),
                    progress = 0f,
                    priority = 1
                )
            )
        }
        return files
    }

    fun updateStatuses() {
        val currentSession = session ?: return
        val torrents = SessionHandle(currentSession.swig()).torrents()
        val statuses = torrents.mapNotNull { handle ->
            if (!handle.isValid) return@mapNotNull null
            
            val status = handle.status()
            val infoHash = handle.infoHash().toHex()
            val torrentInfo = handle.torrentFile()
            val name = torrentInfo?.name() ?: status.name() ?: "Unknown"

            infoHash to TorrentStatus(
                name = name,
                progress = status.progress() * 100f,
                downloadRate = status.downloadRate().toLong(),
                uploadRate = status.uploadRate().toLong(),
                seeders = status.numSeeds(),
                peers = status.numPeers(),
                totalDone = status.totalDone(),
                totalSize = torrentInfo?.totalSize() ?: 0L,
                state = status.state().name,
                infoHash = infoHash
            )
        }.toMap()

        activeTorrents.value = statuses
    }

    fun shutdown() {
        session?.stop()
        session = null
    }

    private fun findHandle(infoHash: String): TorrentHandle? {
        val currentSession = session ?: return null
        return SessionHandle(currentSession.swig()).torrents().find { it.infoHash().toHex() == infoHash }
    }
}
