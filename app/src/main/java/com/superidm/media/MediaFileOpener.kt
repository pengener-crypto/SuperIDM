package com.superidm.media

import android.content.Context
import android.content.Intent
import android.net.Uri
import android.webkit.MimeTypeMap
import androidx.core.content.FileProvider
import dagger.hilt.android.qualifiers.ApplicationContext
import java.io.File
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class MediaFileOpener @Inject constructor(
    @ApplicationContext private val context: Context
) {
    fun getMimeType(file: File): String {
        val ext = MimeTypeMap.getFileExtensionFromUrl(file.name)
        return MimeTypeMap.getSingleton().getMimeTypeFromExtension(ext) ?: "application/octet-stream"
    }
    
    fun getFileProviderUri(file: File): Uri {
        return FileProvider.getUriForFile(context, "${context.packageName}.fileprovider", file)
    }
    
    fun open(file: File, mimeType: String? = null) {
        val type = mimeType ?: getMimeType(file)
        val uri = getFileProviderUri(file)
        val intent = Intent(Intent.ACTION_VIEW).apply {
            setDataAndType(uri, type)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION or Intent.FLAG_ACTIVITY_NEW_TASK)
        }
        context.startActivity(Intent.createChooser(intent, "Open with").apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        })
    }
    
    fun share(file: File, mimeType: String? = null) {
        val type = mimeType ?: getMimeType(file)
        val uri = getFileProviderUri(file)
        val intent = Intent(Intent.ACTION_SEND).apply {
            this.type = type
            putExtra(Intent.EXTRA_STREAM, uri)
            addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
        }
        context.startActivity(Intent.createChooser(intent, "Share via").apply {
            addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
        })
    }
    
    fun canOpenInternally(mimeType: String): Boolean {
        return mimeType.startsWith("video/") || mimeType.startsWith("audio/") ||
               mimeType.startsWith("image/") || mimeType.startsWith("text/")
    }
}
