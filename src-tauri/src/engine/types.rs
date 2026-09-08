use serde::{Deserialize, Serialize};

/// Status of a download task in the SuperIDM engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TaskStatus {
    Queued,
    Probing,
    Downloading,
    Paused,
    Completed,
    Failed,
    Cancelled,
}

/// Metadata for a single byte-range chunk within a segmented download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkInfo {
    pub index: usize,
    pub start_byte: u64,
    pub end_byte: u64,
    pub current_byte: u64,
    pub completed: bool,
    pub retry_count: usize,
}

impl ChunkInfo {
    /// Remaining bytes for this chunk.
    pub fn remaining(&self) -> u64 {
        if self.completed {
            0
        } else {
            self.end_byte.saturating_sub(self.current_byte) + 1
        }
    }

    /// Total size of this chunk in bytes.
    pub fn total_size(&self) -> u64 {
        self.end_byte.saturating_sub(self.start_byte) + 1
    }

    /// How many bytes have been downloaded in this chunk so far.
    pub fn downloaded(&self) -> u64 {
        self.current_byte.saturating_sub(self.start_byte)
    }
}

/// Persistent metadata for a download task, written atomically to `.part.json`.
///
/// Includes ETag and Last-Modified headers captured during the initial probe
/// to validate server consistency on resumption.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DownloadMetadata {
    pub id: String,
    pub url: String,
    pub destination: String,
    pub filename: String,
    pub total_bytes: u64,
    pub status: TaskStatus,
    /// Server ETag from initial probe — used to detect file changes on resume.
    pub etag: Option<String>,
    /// Server Last-Modified from initial probe — secondary consistency check.
    pub last_modified: Option<String>,
    /// Whether the server supports HTTP Range requests.
    pub supports_range: bool,
    /// Number of parallel connections requested.
    pub num_connections: usize,
    /// Priority level (0 = normal, 1 = high).
    pub priority: u8,
    /// Category tag (e.g., "video", "music", "document").
    pub category: String,
    /// Per-chunk state for segmented resume.
    pub chunks: Vec<ChunkInfo>,
    /// Timestamp of task creation (Unix millis).
    pub created_at: i64,
    /// Timestamp of last state update (Unix millis).
    pub updated_at: i64,
}

impl DownloadMetadata {
    /// Total bytes downloaded across all chunks.
    pub fn downloaded_bytes(&self) -> u64 {
        self.chunks.iter().map(|c| c.downloaded()).sum()
    }

    /// Progress fraction 0.0 – 1.0.
    pub fn progress(&self) -> f64 {
        if self.total_bytes == 0 {
            0.0
        } else {
            self.downloaded_bytes() as f64 / self.total_bytes as f64
        }
    }
}

/// Result of probing a URL before starting the download.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProbeResult {
    pub content_length: u64,
    pub supports_range: bool,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub suggested_filename: Option<String>,
    pub content_type: Option<String>,
}

/// Request payload from the frontend to start a new download.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NewDownloadRequest {
    pub url: String,
    #[serde(default)]
    pub destination: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default, alias = "num_connections")]
    pub num_connections: Option<usize>,
    #[serde(default)]
    pub priority: Option<u8>,
    #[serde(default)]
    pub category: Option<String>,
    /// Optional custom HTTP headers (e.g., Authorization, Cookie).
    #[serde(default)]
    pub headers: Option<Vec<(String, String)>>,
}

/// Progress event emitted to the frontend via Tauri events.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressPayload {
    pub id: String,
    pub status: TaskStatus,
    pub downloaded_bytes: u64,
    pub total_bytes: u64,
    pub progress: f64,
    /// Global speed in bytes/sec (EMA-smoothed).
    pub speed: f64,
    /// Per-chunk progress snapshots.
    pub chunks: Vec<ChunkProgressSnapshot>,
    /// Estimated time remaining in seconds.
    pub eta_secs: f64,
}

/// Lightweight per-chunk progress for UI segment rendering.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkProgressSnapshot {
    pub index: usize,
    pub downloaded: u64,
    pub total: u64,
    pub speed: f64,
    pub completed: bool,
}

/// Summary of all download tasks for the frontend dashboard.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskSummary {
    pub id: String,
    pub filename: String,
    pub url: String,
    pub destination: String,
    pub total_bytes: u64,
    pub downloaded_bytes: u64,
    pub progress: f64,
    pub speed: f64,
    pub status: TaskStatus,
    pub category: String,
    pub priority: u8,
    pub num_connections: usize,
    pub created_at: i64,
    pub eta_secs: f64,
}

/// Maximum retries per chunk before marking the task as Failed.
pub const MAX_CHUNK_RETRIES: usize = 5;

/// Base delay for exponential backoff in milliseconds.
pub const RETRY_BASE_DELAY_MS: u64 = 500;

/// Default number of parallel connections per download.
pub const DEFAULT_CONNECTIONS: usize = 8;

/// Maximum number of concurrent download tasks.
pub const MAX_CONCURRENT_DOWNLOADS: usize = 5;

/// Type of download stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamType {
    Direct,
    Hls,
    Dash,
    ExtractedVideo,
}

/// Extracted media quality/format option.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaFormat {
    pub format_id: String,
    pub ext: String,
    pub resolution: String,
    pub note: String,
    pub filesize: Option<u64>,
    pub vcodec: Option<String>,
    pub acodec: Option<String>,
    pub url: Option<String>,
}

/// Result of media extraction for platforms like YouTube, Vimeo, TikTok, etc.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MediaExtractionInfo {
    pub url: String,
    pub title: String,
    pub thumbnail: Option<String>,
    pub duration_secs: Option<u64>,
    pub uploader: Option<String>,
    pub formats: Vec<MediaFormat>,
    pub is_stream: bool,
}

/// A media segment in an HLS (.m3u8) or DASH (.mpd) playlist.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamSegment {
    pub index: usize,
    pub url: String,
    pub duration_secs: f64,
    pub byte_range: Option<(u64, u64)>,
    pub completed: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_new_download_request() {
        let json_camel = r#"{
            "url": "https://proof.ovh.net/files/10Mb.dat",
            "destination": "C:\\Downloads",
            "filename": "10Mb.dat",
            "numConnections": 16,
            "priority": 1,
            "category": "Other",
            "headers": null
        }"#;

        let req: NewDownloadRequest = serde_json::from_str(json_camel).unwrap();
        assert_eq!(req.url, "https://proof.ovh.net/files/10Mb.dat");
        assert_eq!(req.num_connections, Some(16));
        assert_eq!(req.priority, Some(1));
    }

    #[test]
    fn test_deserialize_minimal_request() {
        let json_min = r#"{"url":"https://example.com/test.zip"}"#;
        let req: NewDownloadRequest = serde_json::from_str(json_min).unwrap();
        assert_eq!(req.url, "https://example.com/test.zip");
        assert_eq!(req.destination, "");
        assert_eq!(req.filename, None);
    }
}
