package com.superidm.streaming

import okhttp3.OkHttpClient
import okhttp3.Request
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

data class MediaMetadata(
    val title: String = "",
    val artist: String = "",
    val description: String = "",
    val thumbnailUrl: String = "",
    val durationMs: Long = 0L
)

class MetadataFetcher @Inject constructor(private val okHttpClient: OkHttpClient) {

    suspend fun fetchMetadata(url: String): MediaMetadata = withContext(Dispatchers.IO) {
        try {
            val request = Request.Builder().url(url).head().build()
            val response = okHttpClient.newCall(request).execute()
            
            var title = ""
            val contentDisposition = response.header("Content-Disposition")
            if (contentDisposition != null) {
                val match = Regex("filename=\"([^\"]+)\"").find(contentDisposition)
                if (match != null) {
                    title = match.groupValues[1]
                }
            }
            
            if (title.isEmpty()) {
                title = url.substringAfterLast("/").substringBefore("?")
            }
            
            MediaMetadata(title = title)
        } catch (e: Exception) {
            MediaMetadata()
        }
    }

    suspend fun fetchThumbnailBytes(thumbnailUrl: String): ByteArray? = withContext(Dispatchers.IO) {
        if (thumbnailUrl.isEmpty()) return@withContext null
        try {
            val request = Request.Builder().url(thumbnailUrl).build()
            val response = okHttpClient.newCall(request).execute()
            if (response.isSuccessful) {
                response.body?.bytes()
            } else null
        } catch (e: Exception) {
            null
        }
    }

    fun getOutputFileName(url: String, quality: StreamQuality): String {
        val baseName = url.substringAfterLast("/").substringBefore("?").ifEmpty { "download" }
        val nameWithoutExt = baseName.substringBeforeLast(".")
        return "${nameWithoutExt}_${quality.label}.mp4"
    }
}
