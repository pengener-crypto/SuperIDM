package com.superidm.aria2

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import kotlinx.serialization.KSerializer
import kotlinx.serialization.builtins.ListSerializer
import kotlinx.serialization.builtins.serializer
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.*
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.util.concurrent.atomic.AtomicLong
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class Aria2RpcClient @Inject constructor(private val okHttpClient: OkHttpClient) {
    var serverUrl: String = "http://localhost:6800/jsonrpc"
    var secretToken: String = ""   // aria2c --rpc-secret value
    
    private val json = Json { ignoreUnknownKeys = true; isLenient = true }
    private val requestCounter = AtomicLong(0)
    
    private fun nextId() = requestCounter.incrementAndGet().toString()
    
    private fun tokenParam(): JsonElement = JsonPrimitive("token:$secretToken")

    private suspend fun <T> call(method: String, params: List<JsonElement>, deserializer: KSerializer<T>): T? {
        return withContext(Dispatchers.IO) {
            try {
                val request = RpcRequest(id = nextId(), method = method, params = params)
                val body = json.encodeToString(request).toRequestBody("application/json".toMediaType())
                val httpRequest = Request.Builder().url(serverUrl).post(body).build()
                val response = okHttpClient.newCall(httpRequest).execute()
                if (!response.isSuccessful) return@withContext null
                val responseBody = response.body?.string() ?: return@withContext null
                val rpcResponse = json.decodeFromString(RpcResponse.serializer(deserializer), responseBody)
                rpcResponse.result
            } catch (e: Exception) {
                null
            }
        }
    }

    suspend fun getVersion(): Aria2Version? = call(
        "aria2.getVersion",
        listOf(tokenParam()),
        Aria2Version.serializer()
    )

    suspend fun addUri(urls: List<String>, options: Map<String, String> = emptyMap()): String? = call(
        "aria2.addUri",
        listOf(tokenParam(), JsonArray(urls.map { JsonPrimitive(it) }), JsonObject(options.mapValues { JsonPrimitive(it.value) })),
        String.serializer()
    )

    suspend fun addTorrent(torrentBase64: String, options: Map<String, String> = emptyMap()): String? = call(
        "aria2.addTorrent",
        listOf(tokenParam(), JsonPrimitive(torrentBase64), JsonArray(emptyList()), JsonObject(options.mapValues { JsonPrimitive(it.value) })),
        String.serializer()
    )

    suspend fun addMetalink(metalinkBase64: String): List<String>? = call(
        "aria2.addMetalink",
        listOf(tokenParam(), JsonPrimitive(metalinkBase64)),
        ListSerializer(String.serializer())
    )

    suspend fun remove(gid: String): String? = call(
        "aria2.remove",
        listOf(tokenParam(), JsonPrimitive(gid)),
        String.serializer()
    )

    suspend fun forceRemove(gid: String): String? = call(
        "aria2.forceRemove",
        listOf(tokenParam(), JsonPrimitive(gid)),
        String.serializer()
    )

    suspend fun pause(gid: String): String? = call(
        "aria2.pause",
        listOf(tokenParam(), JsonPrimitive(gid)),
        String.serializer()
    )

    suspend fun pauseAll(): String? = call(
        "aria2.pauseAll",
        listOf(tokenParam()),
        String.serializer()
    )

    suspend fun unpause(gid: String): String? = call(
        "aria2.unpause",
        listOf(tokenParam(), JsonPrimitive(gid)),
        String.serializer()
    )

    suspend fun unpauseAll(): String? = call(
        "aria2.unpauseAll",
        listOf(tokenParam()),
        String.serializer()
    )

    suspend fun tellStatus(gid: String): Aria2DownloadStatus? = call(
        "aria2.tellStatus",
        listOf(tokenParam(), JsonPrimitive(gid)),
        Aria2DownloadStatus.serializer()
    )

    suspend fun tellActive(): List<Aria2DownloadStatus> = call(
        "aria2.tellActive",
        listOf(tokenParam()),
        ListSerializer(Aria2DownloadStatus.serializer())
    ) ?: emptyList()

    suspend fun tellWaiting(offset: Int = 0, num: Int = 100): List<Aria2DownloadStatus> = call(
        "aria2.tellWaiting",
        listOf(tokenParam(), JsonPrimitive(offset), JsonPrimitive(num)),
        ListSerializer(Aria2DownloadStatus.serializer())
    ) ?: emptyList()

    suspend fun tellStopped(offset: Int = -1, num: Int = 100): List<Aria2DownloadStatus> = call(
        "aria2.tellStopped",
        listOf(tokenParam(), JsonPrimitive(offset), JsonPrimitive(num)),
        ListSerializer(Aria2DownloadStatus.serializer())
    ) ?: emptyList()

    suspend fun getGlobalStat(): Aria2GlobalStat? = call(
        "aria2.getGlobalStat",
        listOf(tokenParam()),
        Aria2GlobalStat.serializer()
    )

    suspend fun changeGlobalOption(options: Map<String, String>): String? = call(
        "aria2.changeGlobalOption",
        listOf(tokenParam(), JsonObject(options.mapValues { JsonPrimitive(it.value) })),
        String.serializer()
    )

    suspend fun setSpeedLimit(downloadLimit: Long, uploadLimit: Long) {
        changeGlobalOption(mapOf(
            "max-overall-download-limit" to downloadLimit.toString(),
            "max-overall-upload-limit" to uploadLimit.toString()
        ))
    }

    suspend fun purgeDownloadResult(): String? = call(
        "aria2.purgeDownloadResult",
        listOf(tokenParam()),
        String.serializer()
    )

    suspend fun isReachable(): Boolean {
        return getVersion() != null
    }
}
