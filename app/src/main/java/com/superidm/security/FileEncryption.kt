package com.superidm.security

import android.content.Context
import com.google.crypto.tink.Aead
import com.google.crypto.tink.KeyTemplates
import com.google.crypto.tink.aead.AeadConfig
import com.google.crypto.tink.integration.android.AndroidKeysetManager
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import java.io.File
import java.io.FileInputStream
import java.io.FileOutputStream
import java.nio.ByteBuffer
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class FileEncryption @Inject constructor(
    @ApplicationContext private val context: Context
) {
    companion object {
        private const val KEYSET_NAME = "superidm_enc_keyset"
        private const val PREF_FILE = "superidm_enc_prefs"
        private const val MASTER_KEY_URI = "android-keystore://superidm_master_key"
        const val ENCRYPTED_EXTENSION = ".enc"
        private const val CHUNK_SIZE = 64 * 1024
    }

    init {
        AeadConfig.register()
    }

    private fun getAead(): Aead {
        return AndroidKeysetManager.Builder()
            .withSharedPref(context, KEYSET_NAME, PREF_FILE)
            .withKeyTemplate(KeyTemplates.get("AES256_GCM"))
            .withMasterKeyUri(MASTER_KEY_URI)
            .build()
            .keysetHandle
            .getPrimitive(Aead::class.java)
    }

    suspend fun encryptFile(inputFile: File, outputFile: File) = withContext(Dispatchers.IO) {
        val aead = getAead()
        FileInputStream(inputFile).use { inputStream ->
            FileOutputStream(outputFile).use { outputStream ->
                val buffer = ByteArray(CHUNK_SIZE)
                var bytesRead: Int
                while (inputStream.read(buffer).also { bytesRead = it } != -1) {
                    val chunk = if (bytesRead == CHUNK_SIZE) buffer else buffer.copyOf(bytesRead)
                    val encryptedChunk = aead.encrypt(chunk, ByteArray(0))
                    val lengthBytes = ByteBuffer.allocate(4).putInt(encryptedChunk.size).array()
                    outputStream.write(lengthBytes)
                    outputStream.write(encryptedChunk)
                }
            }
        }
    }

    suspend fun decryptFile(inputFile: File, outputFile: File) = withContext(Dispatchers.IO) {
        val aead = getAead()
        FileInputStream(inputFile).use { inputStream ->
            FileOutputStream(outputFile).use { outputStream ->
                val lengthBuffer = ByteArray(4)
                while (inputStream.read(lengthBuffer) == 4) {
                    val length = ByteBuffer.wrap(lengthBuffer).int
                    val encryptedChunk = ByteArray(length)
                    var totalRead = 0
                    while (totalRead < length) {
                        val read = inputStream.read(encryptedChunk, totalRead, length - totalRead)
                        if (read == -1) break
                        totalRead += read
                    }
                    if (totalRead == length) {
                        val decryptedChunk = aead.decrypt(encryptedChunk, ByteArray(0))
                        outputStream.write(decryptedChunk)
                    }
                }
            }
        }
    }

    suspend fun encryptBytes(data: ByteArray): ByteArray = withContext(Dispatchers.IO) {
        getAead().encrypt(data, ByteArray(0))
    }

    suspend fun decryptBytes(data: ByteArray): ByteArray = withContext(Dispatchers.IO) {
        getAead().decrypt(data, ByteArray(0))
    }

    fun isEncrypted(file: File): Boolean {
        return file.name.endsWith(ENCRYPTED_EXTENSION)
    }
}
