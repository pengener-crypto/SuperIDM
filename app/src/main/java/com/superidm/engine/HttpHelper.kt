package com.superidm.engine

import android.net.Network
import com.superidm.data.db.DownloadEntity
import com.superidm.util.FileUtils
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.json.JSONObject
import java.io.OutputStream
import javax.inject.Inject

data class FileInfo(
    val totalBytes: Long,
    val supportsRanges: Boolean,
    val mimeType: String,
    val suggestedFileName: String,
    val finalUrl: String
)

class HttpHelper @Inject constructor(private val okHttpClient: OkHttpClient) {

    suspend fun probeFile(url: String, entity: DownloadEntity): FileInfo = withContext(Dispatchers.IO) {
        var request = buildRequest(url, entity).newBuilder().head().build()
        var response = okHttpClient.newCall(request).execute()

        if (!response.isSuccessful) {
            response.close()
            request = buildRequest(url, entity).newBuilder().header("Range", "bytes=0-0").get().build()
            response = okHttpClient.newCall(request).execute()
        }

        val totalBytes = response.header("Content-Length")?.toLongOrNull() ?: -1L
        val supportsRanges = response.header("Accept-Ranges") == "bytes" || response.code == 206
        val mimeType = response.header("Content-Type")?.substringBefore(";") ?: "application/octet-stream"
        
        var fileName = ""
        val contentDisposition = response.header("Content-Disposition")
        if (contentDisposition != null && contentDisposition.contains("filename=")) {
            fileName = contentDisposition.substringAfter("filename=").replace("\"", "")
        }
        if (fileName.isEmpty()) {
            fileName = FileUtils.getFileNameFromUrl(response.request.url.toString())
        }

        val finalUrl = response.request.url.toString()
        response.close()

        FileInfo(totalBytes, supportsRanges, mimeType, fileName, finalUrl)
    }

    fun buildRequest(url: String, entity: DownloadEntity, rangeStart: Long = -1, rangeEnd: Long = -1): Request {
        val builder = Request.Builder().url(url)
        
        if (entity.userAgent.isNotEmpty()) {
            builder.header("User-Agent", entity.userAgent)
        }
        if (entity.referrer.isNotEmpty()) {
            builder.header("Referer", entity.referrer)
        }
        
        val headersMap = parseHeaders(entity.headers)
        for ((key, value) in headersMap) {
            builder.header(key, value)
        }

        if (rangeStart >= 0) {
            val rangeEndStr = if (rangeEnd >= 0) rangeEnd.toString() else ""
            builder.header("Range", "bytes=$rangeStart-$rangeEndStr")
        }

        return builder.build()
    }

    fun parseHeaders(headersJson: String): Map<String, String> {
        val map = mutableMapOf<String, String>()
        try {
            if (headersJson.isNotEmpty()) {
                val json = JSONObject(headersJson)
                for (key in json.keys()) {
                    map[key] = json.getString(key)
                }
            }
        } catch (e: Exception) {
            // Ignore
        }
        return map
    }

    suspend fun downloadRange(
        url: String, 
        entity: DownloadEntity, 
        startByte: Long, 
        endByte: Long, 
        outputStream: OutputStream, 
        network: Network? = null,
        onProgress: (Long) -> Unit
    ): Long = withContext(Dispatchers.IO) {
        val request = buildRequest(url, entity, startByte, endByte)
        val client = if (network != null) {
            okHttpClient.newBuilder().socketFactory(network.socketFactory).build()
        } else {
            okHttpClient
        }
        val response = client.newCall(request).execute()
        
        if (!response.isSuccessful) {
            response.close()
            throw Exception("HTTP Error: ${response.code}")
        }

        val body = response.body ?: throw Exception("Empty body")
        var downloaded = 0L
        body.source().inputStream().use { input ->
            val buffer = ByteArray(8192)
            var read: Int
            while (input.read(buffer).also { read = it } != -1) {
                outputStream.write(buffer, 0, read)
                downloaded += read
                onProgress(downloaded)
            }
        }
        downloaded
    }
}
