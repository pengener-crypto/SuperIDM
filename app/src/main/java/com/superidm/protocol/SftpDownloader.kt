package com.superidm.protocol

import com.jcraft.jsch.ChannelSftp
import com.jcraft.jsch.JSch
import com.jcraft.jsch.Session
import com.jcraft.jsch.SftpProgressMonitor
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileOutputStream
import javax.inject.Inject
import javax.inject.Singleton

data class SftpConfig(
    val host: String,
    val port: Int = 22,
    val username: String,
    val password: String = "",
    val privateKeyPath: String = "",
    val passphrase: String = ""
)

data class SftpEntry(
    val name: String,
    val size: Long,
    val isDirectory: Boolean,
    val permissions: String = ""
)

@Singleton
class SftpDownloader @Inject constructor() {

    suspend fun download(
        remotePath: String,
        config: SftpConfig,
        outputFile: File,
        onProgress: (downloaded: Long, total: Long) -> Unit
    ): Boolean = withContext(Dispatchers.IO) {
        val jsch = JSch()
        var session: Session? = null
        var channel: ChannelSftp? = null

        try {
            if (config.privateKeyPath.isNotEmpty()) {
                if (config.passphrase.isNotEmpty()) {
                    jsch.addIdentity(config.privateKeyPath, config.passphrase)
                } else {
                    jsch.addIdentity(config.privateKeyPath)
                }
            }

            session = jsch.getSession(config.username, config.host, config.port)
            if (config.password.isNotEmpty()) {
                session.setPassword(config.password)
            }
            session.setConfig("StrictHostKeyChecking", "no")
            session.connect()

            channel = session.openChannel("sftp") as ChannelSftp
            channel.connect()

            val fileSize = channel.stat(remotePath).size
            val progressMonitor = object : SftpProgressMonitor {
                var downloaded = 0L

                override fun init(op: Int, src: String?, dest: String?, max: Long) {
                    downloaded = 0L
                }

                override fun count(count: Long): Boolean {
                    downloaded += count
                    onProgress(downloaded, fileSize)
                    return true
                }

                override fun end() {}
            }

            FileOutputStream(outputFile).use { output ->
                channel.get(remotePath, output, progressMonitor)
            }

            return@withContext true
        } catch (e: Exception) {
            e.printStackTrace()
            return@withContext false
        } finally {
            channel?.disconnect()
            session?.disconnect()
        }
    }

    suspend fun listDirectory(config: SftpConfig, remotePath: String): List<SftpEntry> = withContext(Dispatchers.IO) {
        val jsch = JSch()
        var session: Session? = null
        var channel: ChannelSftp? = null
        val entries = mutableListOf<SftpEntry>()

        try {
            if (config.privateKeyPath.isNotEmpty()) {
                if (config.passphrase.isNotEmpty()) {
                    jsch.addIdentity(config.privateKeyPath, config.passphrase)
                } else {
                    jsch.addIdentity(config.privateKeyPath)
                }
            }

            session = jsch.getSession(config.username, config.host, config.port)
            if (config.password.isNotEmpty()) {
                session.setPassword(config.password)
            }
            session.setConfig("StrictHostKeyChecking", "no")
            session.connect()

            channel = session.openChannel("sftp") as ChannelSftp
            channel.connect()

            val vector = channel.ls(remotePath)
            for (obj in vector) {
                if (obj is ChannelSftp.LsEntry) {
                    if (obj.filename == "." || obj.filename == "..") continue
                    entries.add(
                        SftpEntry(
                            name = obj.filename,
                            size = obj.attrs.size,
                            isDirectory = obj.attrs.isDir,
                            permissions = obj.attrs.permissionsString
                        )
                    )
                }
            }
        } catch (e: Exception) {
            e.printStackTrace()
        } finally {
            channel?.disconnect()
            session?.disconnect()
        }

        return@withContext entries
    }
}
