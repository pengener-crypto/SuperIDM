package com.superidm.media

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.apache.commons.compress.archivers.tar.TarArchiveEntry
import org.apache.commons.compress.archivers.tar.TarArchiveInputStream
import org.apache.commons.compress.archivers.sevenz.SevenZFile
import org.apache.commons.compress.compressors.gzip.GzipCompressorInputStream
import java.io.BufferedInputStream
import java.io.File
import java.io.FileOutputStream
import java.util.zip.ZipInputStream
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class ArchiveExtractor @Inject constructor(
    @ApplicationContext private val context: Context
) {
    enum class ArchiveType { ZIP, TAR, TAR_GZ, SEVEN_Z, UNKNOWN }
    
    fun detectType(file: File): ArchiveType {
        val ext = file.extension.lowercase()
        return when {
            ext == "zip" -> ArchiveType.ZIP
            ext == "7z" -> ArchiveType.SEVEN_Z
            file.name.endsWith(".tar.gz") || file.name.endsWith(".tgz") -> ArchiveType.TAR_GZ
            ext == "tar" -> ArchiveType.TAR
            else -> ArchiveType.UNKNOWN
        }
    }
    
    private fun validateAndGetFile(destDir: File, entryName: String): File {
        val targetFile = File(destDir, entryName)
        val destCanonical = destDir.canonicalPath
        val targetCanonical = targetFile.canonicalPath
        val prefix = if (destCanonical.endsWith(File.separator)) destCanonical else destCanonical + File.separator
        if (!targetCanonical.startsWith(prefix) && targetCanonical != destCanonical) {
            throw SecurityException("Zip Slip vulnerability attempt detected in archive entry: $entryName")
        }
        return targetFile
    }

    suspend fun extract(archiveFile: File, destinationDir: File, onProgress: (extracted: Long, total: Long) -> Unit): List<File> = withContext(Dispatchers.IO) {
        when (detectType(archiveFile)) {
            ArchiveType.ZIP -> extractZip(archiveFile, destinationDir, onProgress)
            ArchiveType.TAR -> extractTar(archiveFile, destinationDir, false, onProgress)
            ArchiveType.TAR_GZ -> extractTar(archiveFile, destinationDir, true, onProgress)
            ArchiveType.SEVEN_Z -> extract7z(archiveFile, destinationDir, onProgress)
            ArchiveType.UNKNOWN -> emptyList()
        }
    }

    suspend fun extractZip(zipFile: File, dest: File, onProgress: (Long, Long) -> Unit): List<File> = withContext(Dispatchers.IO) {
        val extractedFiles = mutableListOf<File>()
        if (!dest.exists()) dest.mkdirs()
        
        val totalSize = zipFile.length()
        var extractedSize = 0L
        
        ZipInputStream(BufferedInputStream(zipFile.inputStream())).use { zis ->
            var entry = zis.nextEntry
            while (entry != null) {
                val newFile = validateAndGetFile(dest, entry.name)
                
                if (entry.isDirectory) {
                    newFile.mkdirs()
                } else {
                    newFile.parentFile?.mkdirs()
                    FileOutputStream(newFile).use { fos ->
                        val buffer = ByteArray(1024)
                        var len: Int
                        while (zis.read(buffer).also { len = it } > 0) {
                            fos.write(buffer, 0, len)
                            extractedSize += len
                            onProgress(extractedSize, totalSize)
                        }
                    }
                    extractedFiles.add(newFile)
                }
                entry = zis.nextEntry
            }
        }
        extractedFiles
    }

    suspend fun extractTar(tarFile: File, dest: File, gz: Boolean, onProgress: (Long, Long) -> Unit): List<File> = withContext(Dispatchers.IO) {
        val extractedFiles = mutableListOf<File>()
        if (!dest.exists()) dest.mkdirs()
        
        val totalSize = tarFile.length()
        var extractedSize = 0L
        
        val inputStream = if (gz) {
            GzipCompressorInputStream(BufferedInputStream(tarFile.inputStream()))
        } else {
            BufferedInputStream(tarFile.inputStream())
        }
        
        TarArchiveInputStream(inputStream).use { tais ->
            var entry: TarArchiveEntry? = tais.nextEntry as TarArchiveEntry?
            while (entry != null) {
                val newFile = validateAndGetFile(dest, entry.name)
                
                if (entry.isDirectory) {
                    newFile.mkdirs()
                } else {
                    newFile.parentFile?.mkdirs()
                    FileOutputStream(newFile).use { fos ->
                        val buffer = ByteArray(1024)
                        var len: Int
                        while (tais.read(buffer).also { len = it } != -1) {
                            fos.write(buffer, 0, len)
                            extractedSize += len
                            onProgress(extractedSize, totalSize)
                        }
                    }
                    extractedFiles.add(newFile)
                }
                entry = tais.nextEntry as TarArchiveEntry?
            }
        }
        extractedFiles
    }

    suspend fun extract7z(file: File, dest: File, onProgress: (Long, Long) -> Unit): List<File> = withContext(Dispatchers.IO) {
        val extractedFiles = mutableListOf<File>()
        if (!dest.exists()) dest.mkdirs()
        
        val totalSize = file.length()
        var extractedSize = 0L
        
        SevenZFile(file).use { sevenZFile ->
            var entry = sevenZFile.nextEntry
            while (entry != null) {
                val newFile = validateAndGetFile(dest, entry.name)
                
                if (entry.isDirectory) {
                    newFile.mkdirs()
                } else {
                    newFile.parentFile?.mkdirs()
                    FileOutputStream(newFile).use { fos ->
                        val buffer = ByteArray(1024)
                        var len: Int
                        while (sevenZFile.read(buffer).also { len = it } != -1) {
                            fos.write(buffer, 0, len)
                            extractedSize += len
                            onProgress(extractedSize, totalSize)
                        }
                    }
                    extractedFiles.add(newFile)
                }
                entry = sevenZFile.nextEntry
            }
        }
        extractedFiles
    }
}
