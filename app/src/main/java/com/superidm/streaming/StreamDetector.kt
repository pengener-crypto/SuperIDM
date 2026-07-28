package com.superidm.streaming

import okhttp3.OkHttpClient
import okhttp3.Request
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class StreamDetector @Inject constructor(
    private val hlsParser: HlsParser,
    private val dashParser: DashParser,
    private val okHttpClient: OkHttpClient
) {

    suspend fun detect(url: String): StreamInfo? = withContext(Dispatchers.IO) {
        try {
            val request = Request.Builder().url(url).head().build()
            val response = okHttpClient.newCall(request).execute()
            val contentType = response.header("Content-Type") ?: ""

            if (contentType.contains("application/x-mpegURL") || 
                contentType.contains("application/vnd.apple.mpegurl") || 
                url.contains(".m3u8", ignoreCase = true)) {
                return@withContext hlsParser.parseManifest(url)
            }
            
            if (contentType.contains("application/dash+xml") || 
                url.endsWith(".mpd", ignoreCase = true)) {
                return@withContext dashParser.parseManifest(url)
            }
            
            null
        } catch (e: Exception) {
            e.printStackTrace()
            null
        }
    }

    fun isStreamUrl(url: String): Boolean {
        return url.contains(".m3u8", ignoreCase = true) || 
               url.contains(".mpd", ignoreCase = true) || 
               url.contains("manifest", ignoreCase = true)
    }
}
