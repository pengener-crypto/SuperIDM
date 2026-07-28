package com.superidm.streaming

import android.content.Context
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Semaphore
import kotlinx.coroutines.sync.withPermit
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.File
import java.io.FileOutputStream
import javax.inject.Inject
import java.util.concurrent.atomic.AtomicLong

class HlsDownloader @Inject constructor(
    private val hlsParser: HlsParser,
    private val okHttpClient: OkHttpClient,
    @ApplicationContext private val context: Context
) {

    suspend fun download(
        quality: StreamQuality,
        outputDir: File,
        outputFileName: String,
        onProgress: (downloaded: Long, total: Long, speed: Long) -> Unit
    ): File = withContext(Dispatchers.IO) {
        val segments = hlsParser.getSegmentUrls(quality.url)
        val tempDir = File(context.cacheDir, "hls_${System.currentTimeMillis()}")
        tempDir.mkdirs()

        val semaphore = Semaphore(4)
        val downloadedSegments = mutableListOf<File>()
        val totalSegments = segments.size.toLong()
        val downloadedCount = AtomicLong(0)
        
        coroutineScope {
            segments.forEachIndexed { index, segmentUrl ->
                launch {
                    semaphore.withPermit {
                        val segmentFile = File(tempDir, "segment_$index.ts")
                        downloadSegment(segmentUrl, segmentFile)
                        synchronized(downloadedSegments) {
                            downloadedSegments.add(segmentFile)
                        }
                        val current = downloadedCount.incrementAndGet()
                        onProgress(current, totalSegments, 0L)
                    }
                }
            }
        }
        
        val outputFile = File(outputDir, outputFileName)
        val sortedSegments = downloadedSegments.sortedBy { 
            it.nameWithoutExtension.substringAfter("_").toInt() 
        }
        concatenateSegments(sortedSegments, outputFile)
        
        tempDir.deleteRecursively()
        outputFile
    }

    suspend fun downloadSubtitle(subtitleUrl: String, outputPath: String): File? = withContext(Dispatchers.IO) {
        if (subtitleUrl.isEmpty()) return@withContext null
        try {
            val file = File(outputPath)
            downloadSegment(subtitleUrl, file)
            file
        } catch (e: Exception) {
            e.printStackTrace()
            null
        }
    }

    private suspend fun downloadSegment(url: String, outputFile: File) = withContext(Dispatchers.IO) {
        val request = Request.Builder().url(url).build()
        val response = okHttpClient.newCall(request).execute()
        if (!response.isSuccessful) throw Exception("Failed to download segment: ${response.code}")
        
        response.body?.byteStream()?.use { input ->
            FileOutputStream(outputFile).use { output ->
                input.copyTo(output)
            }
        }
    }

    private fun concatenateSegments(segments: List<File>, output: File) {
        FileOutputStream(output).use { out ->
            for (segment in segments) {
                segment.inputStream().use { input ->
                    input.copyTo(out)
                }
            }
        }
    }
}
