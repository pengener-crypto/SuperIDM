package com.superidm.protocol

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import org.xmlpull.v1.XmlPullParser
import org.xmlpull.v1.XmlPullParserFactory
import java.io.File
import java.io.StringReader
import java.security.MessageDigest
import javax.inject.Inject
import javax.inject.Singleton

data class MetalinkFile(
    val name: String,
    val size: Long = 0L,
    val hashType: String = "",
    val hashValue: String = "",
    val mirrors: List<String>
)

@Singleton
class MetalinkParser @Inject constructor(
    private val okHttpClient: OkHttpClient
) {

    suspend fun parse(metalinkUrl: String): List<MetalinkFile> = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(metalinkUrl).build()
        val response = okHttpClient.newCall(request).execute()
        
        if (!response.isSuccessful) {
            return@withContext emptyList()
        }
        
        val xmlContent = response.body?.string() ?: return@withContext emptyList()
        return@withContext parseMeta4Content(xmlContent)
    }

    fun parseMeta4Content(xml: String): List<MetalinkFile> {
        val files = mutableListOf<MetalinkFile>()
        try {
            val factory = XmlPullParserFactory.newInstance()
            factory.isNamespaceAware = true
            val parser = factory.newPullParser()
            parser.setInput(StringReader(xml))

            var eventType = parser.eventType
            var currentName = ""
            var currentSize = 0L
            var currentHashType = ""
            var currentHashValue = ""
            val currentMirrors = mutableListOf<String>()
            
            var inFileName = false
            var inFile = false
            var inSize = false
            var inHash = false
            var inUrl = false

            while (eventType != XmlPullParser.END_DOCUMENT) {
                val tagName = parser.name
                when (eventType) {
                    XmlPullParser.START_TAG -> {
                        when (tagName) {
                            "file" -> {
                                inFile = true
                                currentName = parser.getAttributeValue(null, "name") ?: ""
                                currentSize = 0L
                                currentHashType = ""
                                currentHashValue = ""
                                currentMirrors.clear()
                            }
                            "size" -> inSize = true
                            "hash" -> {
                                inHash = true
                                val type = parser.getAttributeValue(null, "type")
                                if (type == "sha-256" || type == "md5") {
                                    currentHashType = type
                                }
                            }
                            "url" -> inUrl = true
                        }
                    }
                    XmlPullParser.TEXT -> {
                        val text = parser.text.trim()
                        if (text.isNotEmpty()) {
                            if (inFile && inSize) {
                                currentSize = text.toLongOrNull() ?: 0L
                            } else if (inFile && inHash && currentHashType.isNotEmpty()) {
                                currentHashValue = text
                            } else if (inFile && inUrl) {
                                currentMirrors.add(text)
                            }
                        }
                    }
                    XmlPullParser.END_TAG -> {
                        when (tagName) {
                            "file" -> {
                                files.add(
                                    MetalinkFile(
                                        name = currentName,
                                        size = currentSize,
                                        hashType = currentHashType,
                                        hashValue = currentHashValue,
                                        mirrors = currentMirrors.toList()
                                    )
                                )
                                inFile = false
                            }
                            "size" -> inSize = false
                            "hash" -> inHash = false
                            "url" -> inUrl = false
                        }
                    }
                }
                eventType = parser.next()
            }
        } catch (e: Exception) {
            e.printStackTrace()
        }
        return files
    }

    suspend fun benchmarkMirrors(mirrors: List<String>): List<Pair<String, Long>> = withContext(Dispatchers.IO) {
        val jobs = mirrors.map { url ->
            async {
                val start = System.currentTimeMillis()
                val request = Request.Builder()
                    .url(url)
                    .header("Range", "bytes=0-1023")
                    .build()
                
                try {
                    okHttpClient.newCall(request).execute().use { response ->
                        if (response.isSuccessful || response.code == 206) {
                            val duration = System.currentTimeMillis() - start
                            return@async Pair(url, duration)
                        }
                    }
                } catch (e: Exception) {
                    // ignore and let it fall through to worst case
                }
                Pair(url, Long.MAX_VALUE)
            }
        }
        
        jobs.awaitAll()
            .filter { it.second != Long.MAX_VALUE }
            .sortedBy { it.second }
    }

    fun verifyChecksum(file: File, hashType: String, expectedHash: String): Boolean {
        if (!file.exists() || expectedHash.isEmpty()) return false
        try {
            val algorithm = when (hashType.lowercase()) {
                "sha-256" -> "SHA-256"
                "md5" -> "MD5"
                else -> return false
            }
            val md = MessageDigest.getInstance(algorithm)
            file.inputStream().use { input ->
                val buffer = ByteArray(8192)
                var bytesRead: Int
                while (input.read(buffer).also { bytesRead = it } != -1) {
                    md.update(buffer, 0, bytesRead)
                }
            }
            val calculatedHash = md.digest().joinToString("") { "%02x".format(it) }
            return calculatedHash.equals(expectedHash, ignoreCase = true)
        } catch (e: Exception) {
            e.printStackTrace()
            return false
        }
    }
}
