package com.superidm.engine

import android.net.Network
import com.superidm.data.db.DownloadEntity
import com.superidm.data.db.SegmentEntity
import java.io.File
import java.io.FileOutputStream
import javax.inject.Inject

class SegmentDownloader @Inject constructor(
    private val httpHelper: HttpHelper,
    private val retryEngine: RetryEngine,
    private val speedOptimizer: SpeedOptimizer
) {

    suspend fun download(
        entity: DownloadEntity, 
        segment: SegmentEntity, 
        network: Network? = null,
        onProgress: (segmentId: String, bytesDownloaded: Long) -> Unit
    ): Boolean {
        return retryEngine.withRetry {
            val file = File(segment.tempFilePath)
            if (!file.parentFile!!.exists()) file.parentFile!!.mkdirs()
            
            var currentDownloaded = segment.downloadedBytes
            if (file.exists() && file.length() < currentDownloaded) {
                currentDownloaded = file.length()
            }
            if (!file.exists()) {
                currentDownloaded = 0
            }

            val startByte = segment.startByte + currentDownloaded
            if (startByte > segment.endByte && segment.endByte > 0) {
                return@withRetry true // Already complete
            }

            var lastReportTime = System.currentTimeMillis()
            var bytesSinceLastReport = 0L

            FileOutputStream(file, true).use { fos ->
                httpHelper.downloadRange(entity.url, entity, startByte, segment.endByte, fos, network) { downloadedDelta ->
                    val totalDownloaded = currentDownloaded + downloadedDelta
                    
                    val now = System.currentTimeMillis()
                    bytesSinceLastReport += downloadedDelta
                    
                    if (now - lastReportTime >= 500) {
                        speedOptimizer.recordBytes(segment.id, bytesSinceLastReport)
                        bytesSinceLastReport = 0
                        lastReportTime = now
                        onProgress(segment.id, totalDownloaded)
                    }
                }
            }
            
            // Final progress report
            if (bytesSinceLastReport > 0) {
                speedOptimizer.recordBytes(segment.id, bytesSinceLastReport)
            }
            onProgress(segment.id, segment.endByte - segment.startByte + 1)
            
            true
        }
    }
}
