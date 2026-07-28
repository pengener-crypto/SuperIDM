package com.superidm.engine

import android.content.Context
import com.superidm.data.db.DownloadEntity
import com.superidm.data.db.SegmentEntity
import com.superidm.data.model.DownloadStatus
import com.superidm.data.model.SegmentStatus
import com.superidm.data.repository.DownloadRepository
import com.superidm.util.FileUtils
import dagger.hilt.android.qualifiers.ApplicationContext
import kotlinx.coroutines.*
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.SharedFlow
import java.io.File
import java.io.FileOutputStream
import java.util.concurrent.ConcurrentHashMap
import javax.inject.Inject
import kotlin.math.max
import kotlin.math.min

data class DownloadProgress(
    val downloadId: String,
    val downloadedBytes: Long,
    val totalBytes: Long,
    val speed: Long,
    val eta: Long,
    val status: DownloadStatus
)

class DownloadEngine @Inject constructor(
    private val repository: DownloadRepository,
    private val httpHelper: HttpHelper,
    private val segmentDownloader: SegmentDownloader,
    private val retryEngine: RetryEngine,
    private val speedOptimizer: SpeedOptimizer,
    private val dualNetworkBonder: DualNetworkBonder,
    @ApplicationContext private val context: Context
) {

    private val activeJobs = ConcurrentHashMap<String, Job>()
    private val engineScope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val _progressFlow = MutableSharedFlow<DownloadProgress>(replay = 0, extraBufferCapacity = 64)
    val progressFlow: SharedFlow<DownloadProgress> = _progressFlow

    fun startDownload(downloadId: String) {
        if (isActive(downloadId)) return
        activeJobs[downloadId] = engineScope.launch {
            try {
                executeDownload(downloadId)
            } catch (e: CancellationException) {
                // Expected
            } catch (e: Exception) {
                repository.updateStatus(downloadId, DownloadStatus.FAILED)
                repository.updateErrorMessage(downloadId, e.message ?: "Unknown error")
            } finally {
                activeJobs.remove(downloadId)
                speedOptimizer.resetAll()
            }
        }
    }

    fun pauseDownload(downloadId: String) {
        activeJobs[downloadId]?.cancel()
        activeJobs.remove(downloadId)
        engineScope.launch {
            repository.updateStatus(downloadId, DownloadStatus.PAUSED)
        }
    }

    fun resumeDownload(downloadId: String) {
        startDownload(downloadId)
    }

    fun cancelDownload(downloadId: String) {
        activeJobs[downloadId]?.cancel()
        activeJobs.remove(downloadId)
        engineScope.launch {
            repository.updateStatus(downloadId, DownloadStatus.CANCELLED)
            val dir = FileUtils.createTempDir(context, downloadId)
            dir.deleteRecursively()
        }
    }

    fun isActive(downloadId: String): Boolean {
        return activeJobs[downloadId]?.isActive == true
    }

    private suspend fun executeDownload(downloadId: String) {
        val entity = repository.getDownloadById(downloadId) ?: return
        repository.updateStatus(downloadId, DownloadStatus.CONNECTING)

        val info = httpHelper.probeFile(entity.url, entity)
        
        repository.updateDownloadInfo(downloadId, info.totalBytes, info.supportsRanges, info.mimeType, info.suggestedFileName)
        val updatedEntity = repository.getDownloadById(downloadId) ?: return

        if (info.supportsRanges && info.totalBytes > 0) {
            multiSegmentDownload(updatedEntity, info)
        } else {
            singleStreamDownload(updatedEntity, info)
        }
        
        repository.updateStatus(downloadId, DownloadStatus.COMPLETED)
        repository.updateCompletedTime(downloadId, System.currentTimeMillis())
    }

    private suspend fun multiSegmentDownload(entity: DownloadEntity, info: FileInfo) {
        val segmentCount = min(32, max(4, (info.totalBytes / (5 * 1024 * 1024)).toInt()))
        
        var segments = repository.getSegments(entity.id)
        if (segments.isEmpty() || segments.sumOf { it.endByte - it.startByte + 1 } != info.totalBytes) {
            repository.deleteSegments(entity.id)
            val partSize = info.totalBytes / segmentCount
            val newSegments = mutableListOf<SegmentEntity>()
            for (i in 0 until segmentCount) {
                val start = i * partSize
                val end = if (i == segmentCount - 1) info.totalBytes - 1 else start + partSize - 1
                val tempPath = FileUtils.getTempFilePath(context, entity.id, i)
                newSegments.add(SegmentEntity(
                    id = "${entity.id}_$i",
                    downloadId = entity.id,
                    index = i,
                    startByte = start,
                    endByte = end,
                    downloadedBytes = 0,
                    status = SegmentStatus.PENDING,
                    tempFilePath = tempPath,
                    retryCount = 0
                ))
            }
            repository.insertSegments(newSegments)
            segments = newSegments
        }

        repository.updateStatus(entity.id, DownloadStatus.DOWNLOADING)
        var totalDownloadedBytes = segments.sumOf { it.downloadedBytes }

        // Check if Dual-Network Bonding (Wi-Fi + Cellular) is available
        val bondingAvailable = dualNetworkBonder.isBondingAvailable()
        val availableNetworks = if (bondingAvailable) dualNetworkBonder.getAvailableNetworks() else emptyList()

        coroutineScope {
            val progressJob = launch {
                while (isActive) {
                    val speed = speedOptimizer.getTotalSpeed()
                    val eta = speedOptimizer.getEta(info.totalBytes, totalDownloadedBytes)
                    _progressFlow.emit(DownloadProgress(entity.id, totalDownloadedBytes, info.totalBytes, speed, eta, DownloadStatus.DOWNLOADING))
                    delay(500)
                }
            }

            segments.mapIndexed { index, segment ->
                val assignedNetwork = if (availableNetworks.isNotEmpty()) availableNetworks[index % availableNetworks.size].network else null
                async {
                    segmentDownloader.download(entity, segment, assignedNetwork) { segmentId, bytesDownloaded ->
                        totalDownloadedBytes += (bytesDownloaded - segment.downloadedBytes)
                        engineScope.launch {
                            repository.updateSegmentProgress(segmentId, bytesDownloaded)
                        }
                    }
                    repository.updateSegmentStatus(segment.id, SegmentStatus.COMPLETED)
                }
            }.awaitAll()
            
            progressJob.cancel()
        }

        val outputFile = File(entity.saveDir, info.suggestedFileName)
        val segmentFiles = segments.map { File(it.tempFilePath) }
        FileUtils.mergeSegments(segmentFiles, outputFile) { merged ->
            // Optionally report merge progress
        }
        val tempDir = FileUtils.createTempDir(context, entity.id)
        tempDir.deleteRecursively()
    }

    private suspend fun singleStreamDownload(entity: DownloadEntity, info: FileInfo) {
        repository.updateStatus(entity.id, DownloadStatus.DOWNLOADING)
        val outputFile = File(entity.saveDir, info.suggestedFileName)
        
        var downloadedBytes = 0L
        
        coroutineScope {
            val progressJob = launch {
                while (isActive) {
                    val speed = speedOptimizer.getTotalSpeed()
                    val eta = speedOptimizer.getEta(info.totalBytes, downloadedBytes)
                    _progressFlow.emit(DownloadProgress(entity.id, downloadedBytes, info.totalBytes, speed, eta, DownloadStatus.DOWNLOADING))
                    delay(500)
                }
            }
            
            FileOutputStream(outputFile).use { fos ->
                httpHelper.downloadRange(entity.url, entity, 0, -1, fos) { bytes ->
                    downloadedBytes = bytes
                    speedOptimizer.recordBytes(entity.id, bytes)
                }
            }
            progressJob.cancel()
        }
    }
}
