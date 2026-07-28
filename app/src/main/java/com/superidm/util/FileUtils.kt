package com.superidm.util

import android.content.Context
import android.os.StatFs
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.net.URL
import java.security.MessageDigest
import java.security.SecureRandom

object FileUtils {
    suspend fun createTempDir(context: Context, downloadId: String): File = withContext(Dispatchers.IO) {
        val dir = File(context.cacheDir, "superidm_temp/$downloadId")
        if (!dir.exists()) dir.mkdirs()
        dir
    }

    suspend fun getTempFilePath(context: Context, downloadId: String, segmentIndex: Int): String = withContext(Dispatchers.IO) {
        val dir = createTempDir(context, downloadId)
        File(dir, "part_$segmentIndex.tmp").absolutePath
    }

    suspend fun mergeSegments(segmentFiles: List<File>, outputFile: File, onProgress: (Long) -> Unit) = withContext(Dispatchers.IO) {
        var mergedBytes = 0L
        FileOutputStream(outputFile, true).use { outStream ->
            for (file in segmentFiles.sortedBy { it.name }) {
                FileInputStream(file).use { inStream ->
                    val buffer = ByteArray(65536)
                    var bytesRead: Int
                    while (inStream.read(buffer).also { bytesRead = it } != -1) {
                        outStream.write(buffer, 0, bytesRead)
                        mergedBytes += bytesRead
                        onProgress(mergedBytes)
                    }
                }
            }
        }
    }

    suspend fun computeFirstChunkHash(file: File, bytes: Int = 65536): String = withContext(Dispatchers.IO) {
        if (!file.exists()) return@withContext ""
        val digest = MessageDigest.getInstance("SHA-256")
        FileInputStream(file).use { fis ->
            val buffer = ByteArray(bytes)
            val read = fis.read(buffer)
            if (read > 0) {
                digest.update(buffer, 0, read)
            }
        }
        digest.digest().joinToString("") { "%02x".format(it) }
    }

    suspend fun secureDelete(file: File) = withContext(Dispatchers.IO) {
        if (!file.exists()) return@withContext
        val random = SecureRandom()
        val length = file.length()
        try {
            FileOutputStream(file).use { fos ->
                val buffer = ByteArray(4096)
                var written = 0L
                while (written < length) {
                    random.nextBytes(buffer)
                    val toWrite = minOf(4096L, length - written).toInt()
                    fos.write(buffer, 0, toWrite)
                    written += toWrite
                }
            }
        } catch (e: Exception) {
            // ignore
        }
        file.delete()
    }

    suspend fun getAvailableSpace(path: String): Long = withContext(Dispatchers.IO) {
        try {
            val stat = StatFs(path)
            stat.availableBlocksLong * stat.blockSizeLong
        } catch (e: Exception) {
            0L
        }
    }

    suspend fun ensureDirectoryExists(path: String): Boolean = withContext(Dispatchers.IO) {
        val dir = File(path)
        if (!dir.exists()) dir.mkdirs() else true
    }

    fun sanitizeFileName(name: String): String {
        return name.replace(Regex("[\\\\/:*?\"<>|]"), "_")
    }

    fun getFileExtension(url: String): String {
        return try {
            val path = URL(url).path
            val lastDot = path.lastIndexOf('.')
            if (lastDot != -1) path.substring(lastDot + 1) else ""
        } catch (e: Exception) {
            ""
        }
    }

    fun getFileNameFromUrl(url: String): String {
        return try {
            val path = URL(url).path
            val lastSlash = path.lastIndexOf('/')
            val name = if (lastSlash != -1) path.substring(lastSlash + 1) else "downloaded_file"
            if (name.isEmpty()) "downloaded_file" else sanitizeFileName(name)
        } catch (e: Exception) {
            "downloaded_file"
        }
    }
}
