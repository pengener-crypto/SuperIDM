package com.superidm.browser

import android.webkit.URLUtil
import javax.inject.Inject
import javax.inject.Singleton

data class InterceptedDownload(
    val url: String,
    val mimeType: String,
    val contentDisposition: String,
    val contentLength: Long,
    val suggestedFileName: String,
    val cookies: String,
    val userAgent: String,
    val referrer: String
)

@Singleton
class DownloadInterceptor @Inject constructor() {
    private val NON_BROWSABLE_TYPES = setOf(
        "application/octet-stream", "application/zip", "application/x-rar-compressed",
        "application/pdf", "application/vnd.android.package-archive", "application/x-tar",
        "application/gzip", "application/x-7z-compressed", "application/x-apple-diskimage",
        "application/x-iso9660-image"
    )

    private val DOWNLOAD_EXTENSIONS = setOf(
        ".zip", ".rar", ".7z", ".tar", ".gz", ".apk", ".exe", ".dmg", ".iso", ".img"
    )

    fun isDownloadLink(url: String, mimeType: String): Boolean {
        if (NON_BROWSABLE_TYPES.contains(mimeType.lowercase())) return true
        val lowerUrl = url.lowercase()
        return DOWNLOAD_EXTENSIONS.any { lowerUrl.endsWith(it) }
    }

    fun extractFileName(url: String, contentDisposition: String, mimeType: String): String {
        return URLUtil.guessFileName(url, contentDisposition, mimeType)
    }

    fun interceptFromWebView(
        url: String,
        userAgent: String,
        contentDisposition: String,
        mimeType: String,
        contentLength: Long,
        cookies: String,
        referrer: String
    ): InterceptedDownload {
        val fileName = extractFileName(url, contentDisposition, mimeType)
        return InterceptedDownload(
            url = url,
            mimeType = mimeType,
            contentDisposition = contentDisposition,
            contentLength = contentLength,
            suggestedFileName = fileName,
            cookies = cookies,
            userAgent = userAgent,
            referrer = referrer
        )
    }
}
