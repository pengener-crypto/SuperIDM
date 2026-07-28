package com.superidm.aria2

import kotlinx.serialization.SerialName
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.JsonElement

// JSON-RPC request envelope
@Serializable
data class RpcRequest(
    val jsonrpc: String = "2.0",
    val id: String,
    val method: String,
    val params: List<JsonElement>
)

// JSON-RPC response envelope
@Serializable
data class RpcResponse<T>(
    val jsonrpc: String,
    val id: String? = null,
    val result: T? = null,
    val error: RpcError? = null
)

@Serializable
data class RpcError(val code: Int, val message: String)

// aria2c download status (result of aria2.tellStatus)
@Serializable
data class Aria2DownloadStatus(
    val gid: String,
    val status: String,            // active, waiting, paused, error, complete, removed
    val totalLength: String = "0",
    val completedLength: String = "0",
    val downloadSpeed: String = "0",
    val uploadSpeed: String = "0",
    val connections: String = "0",
    val numSeeders: String? = null,
    val errorMessage: String? = null,
    val dir: String = "",
    val files: List<Aria2File> = emptyList(),
    val bittorrent: Aria2Torrent? = null
)

@Serializable
data class Aria2File(
    val index: String,
    val path: String,
    val length: String,
    val completedLength: String,
    val selected: String = "true",
    val uris: List<Aria2Uri> = emptyList()
)

@Serializable
data class Aria2Uri(
    val uri: String,
    val status: String
)

@Serializable
data class Aria2Torrent(
    val announceList: List<List<String>> = emptyList(),
    @SerialName("info") val info: Aria2TorrentInfo? = null
)

@Serializable
data class Aria2TorrentInfo(val name: String = "")

@Serializable
data class Aria2GlobalStat(
    val downloadSpeed: String,
    val uploadSpeed: String,
    val numActive: String,
    val numWaiting: String,
    val numStopped: String,
    val numStoppedTotal: String
)

@Serializable
data class Aria2Version(
    val version: String,
    val enabledFeatures: List<String> = emptyList()
)

// Helper extensions
fun Aria2DownloadStatus.toDownloadStatusEnum(): com.superidm.data.model.DownloadStatus {
    return when (status) {
        "active" -> com.superidm.data.model.DownloadStatus.DOWNLOADING
        "waiting" -> com.superidm.data.model.DownloadStatus.QUEUED
        "paused" -> com.superidm.data.model.DownloadStatus.PAUSED
        "error" -> com.superidm.data.model.DownloadStatus.FAILED
        "complete" -> com.superidm.data.model.DownloadStatus.COMPLETED
        "removed" -> com.superidm.data.model.DownloadStatus.CANCELLED
        else -> com.superidm.data.model.DownloadStatus.QUEUED
    }
}

fun Aria2DownloadStatus.getFileName(): String {
    return bittorrent?.info?.name
        ?: files.firstOrNull()?.path?.substringAfterLast("/")
        ?: files.firstOrNull()?.uris?.firstOrNull()?.uri?.substringAfterLast("/")
        ?: gid
}
