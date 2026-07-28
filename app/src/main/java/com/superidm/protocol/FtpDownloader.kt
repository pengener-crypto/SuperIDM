package com.superidm.protocol

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import org.apache.commons.net.ftp.FTP
import org.apache.commons.net.ftp.FTPClient
import org.apache.commons.net.ftp.FTPSClient
import java.io.File
import java.io.FileOutputStream
import java.net.URI
import javax.inject.Inject
import javax.inject.Singleton

data class FtpConfig(
    val host: String,
    val port: Int = 21,
    val username: String = "anonymous",
    val password: String = "",
    val useFtps: Boolean = false,
    val passiveMode: Boolean = true
)

data class FtpEntry(
    val name: String,
    val size: Long,
    val isDirectory: Boolean,
    val url: String
)

@Singleton
class FtpDownloader @Inject constructor() {

    fun parseFtpUrl(url: String): FtpConfig {
        val uri = URI.create(url)
        val scheme = uri.scheme ?: "ftp"
        val useFtps = scheme.equals("ftps", ignoreCase = true)
        
        var username = "anonymous"
        var password = ""
        
        uri.userInfo?.let { userInfo ->
            val parts = userInfo.split(":", limit = 2)
            if (parts.isNotEmpty()) username = parts[0]
            if (parts.size > 1) password = parts[1]
        }

        val port = if (uri.port != -1) uri.port else if (useFtps) 990 else 21

        return FtpConfig(
            host = uri.host ?: "",
            port = port,
            username = username,
            password = password,
            useFtps = useFtps
        )
    }

    suspend fun download(
        ftpUrl: String,
        outputFile: File,
        resumeFrom: Long = 0L,
        onProgress: (downloaded: Long, total: Long) -> Unit
    ): Boolean = withContext(Dispatchers.IO) {
        val config = parseFtpUrl(ftpUrl)
        val remotePath = URI.create(ftpUrl).path
        val client: FTPClient = if (config.useFtps) FTPSClient() else FTPClient()

        try {
            client.connect(config.host, config.port)
            if (!client.login(config.username, config.password)) {
                return@withContext false
            }

            if (config.passiveMode) {
                client.enterLocalPassiveMode()
            }
            client.setFileType(FTP.BINARY_FILE_TYPE)

            if (resumeFrom > 0) {
                client.setRestartOffset(resumeFrom)
            }

            val files = client.listFiles(remotePath)
            val fileSize = if (files.isNotEmpty()) files[0].size else -1L

            val inputStream = client.retrieveFileStream(remotePath) ?: return@withContext false
            val outputStream = FileOutputStream(outputFile, resumeFrom > 0)

            inputStream.use { input ->
                outputStream.use { output ->
                    val buffer = ByteArray(8192)
                    var bytesRead: Int
                    var totalDownloaded = resumeFrom

                    while (input.read(buffer).also { bytesRead = it } != -1) {
                        output.write(buffer, 0, bytesRead)
                        totalDownloaded += bytesRead
                        onProgress(totalDownloaded, fileSize)
                    }
                }
            }

            client.completePendingCommand()
            return@withContext true
        } catch (e: Exception) {
            e.printStackTrace()
            return@withContext false
        } finally {
            if (client.isConnected) {
                try {
                    client.logout()
                    client.disconnect()
                } catch (e: Exception) {
                    // Ignore disconnect errors
                }
            }
        }
    }

    suspend fun listDirectory(ftpUrl: String): List<FtpEntry> = withContext(Dispatchers.IO) {
        val config = parseFtpUrl(ftpUrl)
        val remotePath = URI.create(ftpUrl).path.ifEmpty { "/" }
        val client: FTPClient = if (config.useFtps) FTPSClient() else FTPClient()
        val entries = mutableListOf<FtpEntry>()

        try {
            client.connect(config.host, config.port)
            client.login(config.username, config.password)
            if (config.passiveMode) client.enterLocalPassiveMode()

            val files = client.listFiles(remotePath)
            for (file in files) {
                val fileUrl = if (ftpUrl.endsWith("/")) ftpUrl + file.name else "$ftpUrl/${file.name}"
                entries.add(
                    FtpEntry(
                        name = file.name,
                        size = file.size,
                        isDirectory = file.isDirectory,
                        url = fileUrl
                    )
                )
            }
        } catch (e: Exception) {
            e.printStackTrace()
        } finally {
            if (client.isConnected) {
                try {
                    client.logout()
                    client.disconnect()
                } catch (e: Exception) {
                    // Ignore
                }
            }
        }
        return@withContext entries
    }
}
