package com.superidm.streaming

data class StreamQuality(
    val label: String,
    val width: Int = 0,
    val height: Int = 0,
    val bitrate: Long = 0L,
    val codecs: String = "",
    val url: String,
    val isAudioOnly: Boolean = false,
    val subtitleUrl: String = "",
    val subtitleLanguage: String = ""
)

data class StreamInfo(
    val originalUrl: String,
    val type: StreamType,
    val qualities: List<StreamQuality>,
    val title: String = "",
    val thumbnail: String = "",
    val duration: Long = 0L
)

enum class StreamType { HLS, DASH, DIRECT }
