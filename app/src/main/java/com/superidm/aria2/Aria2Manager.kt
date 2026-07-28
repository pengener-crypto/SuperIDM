package com.superidm.aria2

import android.util.Base64
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.core.stringPreferencesKey
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.*
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class Aria2Manager @Inject constructor(
    private val client: Aria2RpcClient,
    private val dataStore: DataStore<Preferences>
) {
    companion object {
        private val KEY_SERVER_URL = stringPreferencesKey("aria2_server_url")
        private val KEY_SECRET = stringPreferencesKey("aria2_secret")
        private const val DEFAULT_URL = "http://localhost:6800/jsonrpc"
    }

    val isConnected = MutableStateFlow(false)
    val globalStat = MutableStateFlow<Aria2GlobalStat?>(null)
    val allDownloads = MutableStateFlow<List<Aria2DownloadStatus>>(emptyList())

    val serverUrl: Flow<String> = dataStore.data.map { it[KEY_SERVER_URL] ?: DEFAULT_URL }
    val secretToken: Flow<String> = dataStore.data.map { it[KEY_SECRET] ?: "" }

    private var pollingJob: Job? = null
    private val scope = CoroutineScope(Dispatchers.IO + SupervisorJob())

    suspend fun saveConfig(url: String, secret: String) {
        dataStore.edit { preferences ->
            preferences[KEY_SERVER_URL] = url
            preferences[KEY_SECRET] = secret
        }
        client.serverUrl = url
        client.secretToken = secret
    }

    suspend fun loadConfig() {
        val url = serverUrl.first()
        val secret = secretToken.first()
        client.serverUrl = url
        client.secretToken = secret
    }

    fun startPolling(intervalMs: Long = 2000L) {
        if (pollingJob?.isActive == true) return
        pollingJob = scope.launch {
            while (isActive) {
                val reachable = client.isReachable()
                isConnected.value = reachable
                
                if (reachable) {
                    val active = client.tellActive()
                    val waiting = client.tellWaiting(0, 100)
                    val stopped = client.tellStopped(-1, 100)
                    allDownloads.value = active + waiting + stopped
                    
                    globalStat.value = client.getGlobalStat()
                } else {
                    globalStat.value = null
                    allDownloads.value = emptyList()
                }
                delay(intervalMs)
            }
        }
    }

    fun stopPolling() {
        pollingJob?.cancel()
        pollingJob = null
    }

    suspend fun addUri(url: String, savePath: String = "", filename: String = ""): String? {
        val options = mutableMapOf<String, String>()
        if (savePath.isNotEmpty()) options["dir"] = savePath
        if (filename.isNotEmpty()) options["out"] = filename
        return client.addUri(listOf(url), options)
    }

    suspend fun addTorrentFile(torrentBytes: ByteArray, savePath: String = ""): String? {
        val options = mutableMapOf<String, String>()
        if (savePath.isNotEmpty()) options["dir"] = savePath
        val base64 = Base64.encodeToString(torrentBytes, Base64.NO_WRAP)
        return client.addTorrent(base64, options)
    }

    suspend fun pause(gid: String) = client.pause(gid)
    suspend fun resume(gid: String) = client.unpause(gid)
    suspend fun remove(gid: String) = client.remove(gid)
    suspend fun forceRemove(gid: String) = client.forceRemove(gid)
    
    suspend fun pauseAll() = client.pauseAll()
    suspend fun resumeAll() = client.unpauseAll()
    suspend fun purgeCompleted() = client.purgeDownloadResult()

    suspend fun setSpeedLimit(downloadMbps: Float, uploadMbps: Float) {
        val downBytes = (downloadMbps * 1024 * 1024).toLong()
        val upBytes = (uploadMbps * 1024 * 1024).toLong()
        client.setSpeedLimit(downBytes, upBytes)
    }

    suspend fun testConnection(): Boolean {
        val reachable = client.isReachable()
        isConnected.value = reachable
        return reachable
    }
}
