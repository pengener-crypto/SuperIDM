package com.superidm.data.repository

import com.superidm.data.db.DownloadDao
import com.superidm.data.db.DownloadEntity
import com.superidm.data.db.SegmentDao
import com.superidm.data.db.SegmentEntity
import com.superidm.data.model.DownloadPriority
import com.superidm.data.model.DownloadStatus
import com.superidm.data.model.SegmentStatus
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.withContext
import java.util.UUID
import javax.inject.Inject
import javax.inject.Singleton

@Singleton
class DownloadRepository @Inject constructor(
    private val downloadDao: DownloadDao,
    private val segmentDao: SegmentDao
) {

    fun getAllDownloads(): Flow<List<DownloadEntity>> = downloadDao.getAll()

    fun getActiveDownloads(): Flow<List<DownloadEntity>> = downloadDao.getActive()

    fun getCompletedDownloads(): Flow<List<DownloadEntity>> = downloadDao.getCompleted()

    fun searchDownloads(query: String): Flow<List<DownloadEntity>> = downloadDao.search(query)

    fun getDownloadsByCategory(category: String): Flow<List<DownloadEntity>> = downloadDao.getByCategory(category)

    suspend fun insertDownload(entity: DownloadEntity) = withContext(Dispatchers.IO) {
        downloadDao.insert(entity)
    }

    // Short alias for convenience
    suspend fun insert(entity: DownloadEntity) = insertDownload(entity)

    suspend fun updateDownload(entity: DownloadEntity) = withContext(Dispatchers.IO) {
        downloadDao.update(entity)
    }

    suspend fun deleteDownload(entity: DownloadEntity) = withContext(Dispatchers.IO) {
        downloadDao.delete(entity)
    }

    suspend fun deleteDownloadById(id: String) = withContext(Dispatchers.IO) {
        downloadDao.deleteById(id)
    }

    suspend fun getDownloadById(id: String): DownloadEntity? = withContext(Dispatchers.IO) {
        downloadDao.getById(id)
    }

    suspend fun getDownloadsByStatus(statuses: List<DownloadStatus>): List<DownloadEntity> = withContext(Dispatchers.IO) {
        downloadDao.getByStatuses(statuses)
    }

    suspend fun updateDownloadStatus(id: String, status: DownloadStatus) = withContext(Dispatchers.IO) {
        downloadDao.updateStatus(id, status)
    }

    // Short alias used by DownloadEngine
    suspend fun updateStatus(id: String, status: DownloadStatus) = updateDownloadStatus(id, status)

    suspend fun updatePriority(id: String, priority: DownloadPriority) = withContext(Dispatchers.IO) {
        downloadDao.updatePriority(id, priority)
    }

    suspend fun updateErrorMessage(id: String, message: String) = withContext(Dispatchers.IO) {
        downloadDao.updateErrorMessage(id, message)
    }

    suspend fun updateDownloadInfo(
        id: String,
        totalBytes: Long,
        resumable: Boolean,
        mimeType: String,
        fileName: String
    ) = withContext(Dispatchers.IO) {
        downloadDao.updateDownloadInfo(id, totalBytes, resumable, mimeType, fileName)
    }

    suspend fun updateCompletedTime(id: String, completedAt: Long) = withContext(Dispatchers.IO) {
        downloadDao.updateCompletedTime(id, completedAt)
    }

    suspend fun updateDownloadProgress(id: String, downloadedBytes: Long, speed: Long) = withContext(Dispatchers.IO) {
        downloadDao.updateProgress(id, downloadedBytes, speed)
    }

    suspend fun updateDownloadCompletion(id: String, completedAt: Long, averageSpeed: Long) = withContext(Dispatchers.IO) {
        downloadDao.updateCompletion(id, completedAt, averageSpeed)
    }

    suspend fun updateDownloadResumable(id: String, resumable: Boolean, totalBytes: Long) = withContext(Dispatchers.IO) {
        downloadDao.updateResumable(id, resumable, totalBytes)
    }

    suspend fun countDownloadsByStatus(status: DownloadStatus): Int = withContext(Dispatchers.IO) {
        downloadDao.countByStatus(status)
    }

    suspend fun insertSegments(segments: List<SegmentEntity>) = withContext(Dispatchers.IO) {
        segmentDao.insertAll(segments)
    }

    suspend fun insertSegment(segment: SegmentEntity) = withContext(Dispatchers.IO) {
        segmentDao.insert(segment)
    }

    suspend fun getSegmentsByDownloadId(downloadId: String): List<SegmentEntity> = withContext(Dispatchers.IO) {
        segmentDao.getByDownloadId(downloadId)
    }

    // Short alias used by DownloadEngine
    suspend fun getSegments(downloadId: String): List<SegmentEntity> = getSegmentsByDownloadId(downloadId)

    // 2-arg overload used by DownloadEngine progress callback
    suspend fun updateSegmentProgress(id: String, downloadedBytes: Long) = withContext(Dispatchers.IO) {
        segmentDao.updateProgress(id, downloadedBytes, SegmentStatus.DOWNLOADING)
    }

    suspend fun updateSegmentProgress(id: String, downloadedBytes: Long, status: SegmentStatus) = withContext(Dispatchers.IO) {
        segmentDao.updateProgress(id, downloadedBytes, status)
    }

    suspend fun updateSegmentStatus(id: String, status: SegmentStatus) = withContext(Dispatchers.IO) {
        segmentDao.updateStatus(id, status)
    }

    suspend fun deleteSegmentsByDownloadId(downloadId: String) = withContext(Dispatchers.IO) {
        segmentDao.deleteByDownloadId(downloadId)
    }

    // Short alias used by DownloadEngine
    suspend fun deleteSegments(downloadId: String) = deleteSegmentsByDownloadId(downloadId)

    suspend fun getCompletedSegmentCount(downloadId: String): Int = withContext(Dispatchers.IO) {
        segmentDao.getCompletedCount(downloadId)
    }

    suspend fun incrementSegmentRetry(id: String) = withContext(Dispatchers.IO) {
        segmentDao.incrementRetry(id)
    }

    fun generateId(): String {
        return UUID.randomUUID().toString()
    }

    fun calculateCategory(mimeType: String, url: String): String {
        val lowerMime = mimeType.lowercase()
        val lowerUrl = url.lowercase()

        return when {
            lowerMime.startsWith("video/") || lowerUrl.endsWith(".mp4") || lowerUrl.endsWith(".mkv") || lowerUrl.endsWith(".avi") -> "video"
            lowerMime.startsWith("audio/") || lowerUrl.endsWith(".mp3") || lowerUrl.endsWith(".wav") || lowerUrl.endsWith(".flac") -> "audio"
            lowerMime.startsWith("image/") || lowerUrl.endsWith(".jpg") || lowerUrl.endsWith(".jpeg") || lowerUrl.endsWith(".png") || lowerUrl.endsWith(".gif") -> "image"
            lowerMime.contains("pdf") || lowerMime.contains("document") || lowerMime.contains("msword") || lowerMime.contains("excel") || lowerUrl.endsWith(".pdf") || lowerUrl.endsWith(".doc") || lowerUrl.endsWith(".docx") || lowerUrl.endsWith(".txt") -> "document"
            lowerMime.contains("vnd.android.package-archive") || lowerUrl.endsWith(".apk") -> "apk"
            lowerMime.contains("zip") || lowerMime.contains("rar") || lowerMime.contains("x-tar") || lowerUrl.endsWith(".zip") || lowerUrl.endsWith(".rar") || lowerUrl.endsWith(".tar.gz") || lowerUrl.endsWith(".7z") -> "archive"
            else -> "other"
        }
    }
}
