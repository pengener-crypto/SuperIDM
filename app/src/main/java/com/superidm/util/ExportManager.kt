package com.superidm.util

import android.content.Context
import android.net.Uri
import androidx.core.content.FileProvider
import com.superidm.data.db.DownloadEntity
import com.superidm.data.repository.DownloadRepository
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.flow.first
import java.io.File
import java.io.FileWriter
import java.text.SimpleDateFormat
import java.util.Date
import java.util.Locale
import javax.inject.Inject
import javax.inject.Singleton

enum class ExportFormat { CSV, JSON }

@Singleton
class ExportManager @Inject constructor(
    private val repository: DownloadRepository,
    @ApplicationContext private val context: Context
) {

    suspend fun exportHistory(format: ExportFormat, outputFile: File): File {
        val downloads = repository.getAllDownloads().first()
        
        FileWriter(outputFile).use { writer ->
            when (format) {
                ExportFormat.CSV -> {
                    writer.write("id,url,fileName,totalBytes,downloadedBytes,status,mimeType,categoryFolder,createdAt,completedAt,averageSpeed\n")
                    for (download in downloads) {
                        writer.write(getCsvRow(download) + "\n")
                    }
                }
                ExportFormat.JSON -> {
                    writer.write("[\n")
                    downloads.forEachIndexed { index, download ->
                        val json = buildJsonString(download)
                        writer.write("  $json")
                        if (index < downloads.size - 1) {
                            writer.write(",\n")
                        } else {
                            writer.write("\n")
                        }
                    }
                    writer.write("]\n")
                }
            }
        }
        return outputFile
    }

    suspend fun exportToShareable(format: ExportFormat): Uri {
        val extension = if (format == ExportFormat.CSV) "csv" else "json"
        val fileName = "superidm_history_${System.currentTimeMillis()}.$extension"
        val cacheDir = File(context.cacheDir, "exports").apply { mkdirs() }
        val outputFile = File(cacheDir, fileName)
        
        val exportedFile = exportHistory(format, outputFile)
        
        return FileProvider.getUriForFile(
            context,
            "${context.packageName}.fileprovider",
            exportedFile
        )
    }

    fun getCsvRow(entity: DownloadEntity): String {
        return buildString {
            append(entity.id).append(",")
            append("\"").append(entity.url.replace("\"", "\"\"")).append("\",")
            append("\"").append(entity.fileName.replace("\"", "\"\"")).append("\",")
            append(entity.totalBytes).append(",")
            append(entity.downloadedBytes).append(",")
            append(entity.status.name).append(",")
            append("\"").append(entity.mimeType?.replace("\"", "\"\"") ?: "").append("\",")
            append("\"").append(entity.categoryFolder?.replace("\"", "\"\"") ?: "").append("\",")
            append(formatTimestamp(entity.createdAt)).append(",")
            append(entity.completedAt?.let { formatTimestamp(it) } ?: "").append(",")
            append(entity.averageSpeed ?: 0)
        }
    }

    fun formatTimestamp(ms: Long): String {
        val sdf = SimpleDateFormat("yyyy-MM-dd'T'HH:mm:ss'Z'", Locale.US)
        return sdf.format(Date(ms))
    }

    private fun buildJsonString(entity: DownloadEntity): String {
        // Simple manual JSON builder for exact compliance without external serialization libs
        return """
            {
              "id": ${entity.id},
              "url": "${entity.url.replace("\"", "\\\"")}",
              "fileName": "${entity.fileName.replace("\"", "\\\"")}",
              "totalBytes": ${entity.totalBytes},
              "downloadedBytes": ${entity.downloadedBytes},
              "status": "${entity.status.name}",
              "mimeType": ${entity.mimeType?.let { "\"${it.replace("\"", "\\\"")}\"" } ?: "null"},
              "categoryFolder": ${entity.categoryFolder?.let { "\"${it.replace("\"", "\\\"")}\"" } ?: "null"},
              "createdAt": "${formatTimestamp(entity.createdAt)}",
              "completedAt": ${entity.completedAt?.let { "\"${formatTimestamp(it)}\"" } ?: "null"},
              "averageSpeed": ${entity.averageSpeed ?: 0}
            }
        """.trimIndent()
    }
}
