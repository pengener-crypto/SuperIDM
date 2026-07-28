package com.superidm.streaming

import okhttp3.OkHttpClient
import okhttp3.Request
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class HlsParser @Inject constructor(private val okHttpClient: OkHttpClient) {

    suspend fun parseManifest(url: String): StreamInfo = withContext(Dispatchers.IO) {
        val content = fetchContent(url)
        val qualities = mutableListOf<StreamQuality>()

        if (content.contains("#EXT-X-STREAM-INF")) {
            val lines = content.lines()
            var currentResolution = ""
            var currentBandwidth = 0L
            var currentCodecs = ""
            
            for (i in lines.indices) {
                val line = lines[i].trim()
                if (line.startsWith("#EXT-X-STREAM-INF:")) {
                    val bandwidthMatch = Regex("BANDWIDTH=(\\d+)").find(line)
                    if (bandwidthMatch != null) {
                        currentBandwidth = bandwidthMatch.groupValues[1].toLong()
                    }
                    val resolutionMatch = Regex("RESOLUTION=(\\d+x\\d+)").find(line)
                    if (resolutionMatch != null) {
                        currentResolution = resolutionMatch.groupValues[1]
                    }
                    val codecsMatch = Regex("CODECS=\"([^\"]+)\"").find(line)
                    if (codecsMatch != null) {
                        currentCodecs = codecsMatch.groupValues[1]
                    }
                } else if (line.isNotEmpty() && !line.startsWith("#")) {
                    val streamUrl = resolveUrl(url, line)
                    val width = currentResolution.split("x").getOrNull(0)?.toIntOrNull() ?: 0
                    val height = currentResolution.split("x").getOrNull(1)?.toIntOrNull() ?: 0
                    qualities.add(
                        StreamQuality(
                            label = "${height}p",
                            width = width,
                            height = height,
                            bitrate = currentBandwidth,
                            codecs = currentCodecs,
                            url = streamUrl
                        )
                    )
                    // Reset for next
                    currentResolution = ""
                    currentBandwidth = 0L
                    currentCodecs = ""
                }
            }
            
            // Look for audio only tracks
            val audioLines = lines.filter { it.startsWith("#EXT-X-MEDIA:TYPE=AUDIO") }
            for (audioLine in audioLines) {
                val uriMatch = Regex("URI=\"([^\"]+)\"").find(audioLine)
                if (uriMatch != null) {
                    val audioUrl = resolveUrl(url, uriMatch.groupValues[1])
                    val nameMatch = Regex("NAME=\"([^\"]+)\"").find(audioLine)
                    val name = nameMatch?.groupValues?.get(1) ?: "Audio"
                    qualities.add(
                        StreamQuality(
                            label = name,
                            url = audioUrl,
                            isAudioOnly = true
                        )
                    )
                }
            }
            
        } else if (content.contains("#EXTINF")) {
            qualities.add(
                StreamQuality(
                    label = "Original",
                    url = url
                )
            )
        }

        StreamInfo(
            originalUrl = url,
            type = StreamType.HLS,
            qualities = qualities
        )
    }

    suspend fun getSegmentUrls(qualityUrl: String): List<String> = withContext(Dispatchers.IO) {
        val content = fetchContent(qualityUrl)
        val urls = mutableListOf<String>()
        val lines = content.lines()
        for (line in lines) {
            val tLine = line.trim()
            if (tLine.isNotEmpty() && !tLine.startsWith("#")) {
                urls.add(resolveUrl(qualityUrl, tLine))
            }
        }
        urls
    }

    fun resolveUrl(baseUrl: String, path: String): String {
        return if (path.startsWith("http://") || path.startsWith("https://")) {
            path
        } else {
            val basePath = baseUrl.substringBeforeLast("/")
            "$basePath/$path"
        }
    }

    private suspend fun fetchContent(url: String): String = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(url).build()
        val response = okHttpClient.newCall(request).execute()
        if (!response.isSuccessful) {
            throw Exception("Failed to fetch content: ${response.code}")
        }
        response.body?.string() ?: throw Exception("Empty response body")
    }
}
