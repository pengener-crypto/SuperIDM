package com.superidm.util

object CategoryUtils {
    private val mimeCategories = mapOf(
        "video/" to "video",
        "audio/" to "audio",
        "image/" to "image",
        "application/pdf" to "document",
        "application/msword" to "document",
        "application/vnd." to "document",
        "text/" to "document",
        "application/vnd.android.package-archive" to "apk",
        "application/zip" to "archive",
        "application/x-rar-compressed" to "archive",
        "application/x-tar" to "archive",
        "application/x-7z-compressed" to "archive"
    )

    private val extensionCategories = mapOf(
        "mp4" to "video", "mkv" to "video", "avi" to "video",
        "mp3" to "audio", "wav" to "audio", "flac" to "audio",
        "jpg" to "image", "jpeg" to "image", "png" to "image", "gif" to "image",
        "pdf" to "document", "doc" to "document", "docx" to "document", "txt" to "document",
        "apk" to "apk",
        "zip" to "archive", "rar" to "archive", "7z" to "archive", "tar" to "archive", "gz" to "archive"
    )

    fun getCategory(mimeType: String, url: String): String {
        for ((prefix, category) in mimeCategories) {
            if (mimeType.lowercase().startsWith(prefix)) return category
        }
        val ext = FileUtils.getFileExtension(url).lowercase()
        return extensionCategories[ext] ?: "other"
    }

    fun getCategoryFolder(category: String, baseDir: String): String {
        val folder = when (category) {
            "video" -> "Videos"
            "audio" -> "Audio"
            "image" -> "Images"
            "document" -> "Documents"
            "apk" -> "Apps"
            "archive" -> "Archives"
            else -> "Others"
        }
        return "$baseDir/$folder"
    }

    fun getCategoryIcon(category: String): Int {
        return 0 // Placeholder
    }
}
