package com.superidm.streaming

import okhttp3.OkHttpClient
import okhttp3.Request
import org.xmlpull.v1.XmlPullParser
import org.xmlpull.v1.XmlPullParserFactory
import java.io.StringReader
import javax.inject.Inject
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

class DashParser @Inject constructor(private val okHttpClient: OkHttpClient) {

    suspend fun parseManifest(url: String): StreamInfo = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(url).build()
        val response = okHttpClient.newCall(request).execute()
        val xmlContent = response.body?.string() ?: throw Exception("Empty DASH manifest")

        val factory = XmlPullParserFactory.newInstance()
        factory.isNamespaceAware = true
        val parser = factory.newPullParser()
        parser.setInput(StringReader(xmlContent))

        val qualities = mutableListOf<StreamQuality>()
        var eventType = parser.eventType
        
        var currentContentType = ""
        var currentMimeType = ""

        while (eventType != XmlPullParser.END_DOCUMENT) {
            val tagName = parser.name
            when (eventType) {
                XmlPullParser.START_TAG -> {
                    if (tagName.equals("AdaptationSet", ignoreCase = true)) {
                        currentContentType = parser.getAttributeValue(null, "contentType") ?: ""
                        currentMimeType = parser.getAttributeValue(null, "mimeType") ?: ""
                    } else if (tagName.equals("Representation", ignoreCase = true)) {
                        val bandwidth = parser.getAttributeValue(null, "bandwidth")?.toLongOrNull() ?: 0L
                        val width = parser.getAttributeValue(null, "width")?.toIntOrNull() ?: 0
                        val height = parser.getAttributeValue(null, "height")?.toIntOrNull() ?: 0
                        val codecs = parser.getAttributeValue(null, "codecs") ?: ""
                        val repMimeType = parser.getAttributeValue(null, "mimeType") ?: currentMimeType
                        
                        val isAudio = isAudioMimeType(repMimeType) || currentContentType == "audio"
                        
                        val label = if (isAudio) {
                            "Audio ${bandwidth / 1000}kbps"
                        } else {
                            if (height > 0) "${height}p" else "Video"
                        }

                        qualities.add(
                            StreamQuality(
                                label = label,
                                width = width,
                                height = height,
                                bitrate = bandwidth,
                                codecs = codecs,
                                url = url, 
                                isAudioOnly = isAudio
                            )
                        )
                    }
                }
                XmlPullParser.END_TAG -> {
                    if (tagName.equals("AdaptationSet", ignoreCase = true)) {
                        currentContentType = ""
                        currentMimeType = ""
                    }
                }
            }
            eventType = parser.next()
        }

        StreamInfo(
            originalUrl = url,
            type = StreamType.DASH,
            qualities = qualities
        )
    }

    fun isAudioMimeType(mimeType: String): Boolean {
        return mimeType.startsWith("audio/") || mimeType.contains("audio", ignoreCase = true)
    }
}
