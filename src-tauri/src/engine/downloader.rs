use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};

use super::speed::SpeedMeter;
use super::storage;
use super::throttler::BandwidthThrottler;
use super::types::*;

/// Internal event sent from chunk workers to the download coordinator.
/// Internal event sent from chunk workers to the download coordinator.
#[derive(Debug, Clone)]
pub enum WorkerEvent {
    /// Chunk received bytes at the given offset.
    Progress {
        chunk_index: usize,
        bytes_written: u64,
        new_offset: u64,
    },
    /// Chunk completed successfully.
    ChunkDone {
        chunk_index: usize,
    },
    /// Chunk encountered an error (will retry or fail).
    ChunkError {
        chunk_index: usize,
        error: String,
        will_retry: bool,
    },
}

/// The core download manager holding all active/paused/queued tasks.
pub struct DownloadManager {
    pub tasks: HashMap<String, TaskHandle>,
    pub http_client: Client,
    pub throttler: BandwidthThrottler,
    pub default_download_dir: PathBuf,
    pub max_concurrent: usize,
    /// Queue of download requests received from the browser extension via RPC.
    pub pending_rpc_downloads: Vec<crate::engine::rpc::RpcDownloadRequest>,
}

/// Handle to an active download task, holding its metadata and cancellation token.
pub struct TaskHandle {
    pub metadata: DownloadMetadata,
    pub speed_meter: SpeedMeter,
    /// Cancellation signal — set to `true` to pause/cancel chunk workers.
    pub cancel_token: watch::Sender<bool>,
}

const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

impl DownloadManager {
    pub fn new(download_dir: PathBuf) -> Self {
        let client = Client::builder()
            .user_agent(DEFAULT_USER_AGENT)
            .redirect(reqwest::redirect::Policy::limited(15))
            .connect_timeout(Duration::from_secs(15))
            .pool_max_idle_per_host(32)
            .pool_idle_timeout(Some(Duration::from_secs(90)))
            .tcp_keepalive(Some(Duration::from_secs(60)))
            .build()
            .expect("Failed to build HTTP client");

        Self {
            tasks: HashMap::new(),
            http_client: client,
            throttler: BandwidthThrottler::new(0), // unlimited
            default_download_dir: download_dir,
            max_concurrent: MAX_CONCURRENT_DOWNLOADS,
            pending_rpc_downloads: Vec::new(),
        }
    }

    /// Get the number of currently active (downloading) tasks.
    pub fn active_count(&self) -> usize {
        self.tasks
            .iter()
            .filter(|(_, h)| h.metadata.status == TaskStatus::Downloading)
            .count()
    }
}

/// Probe a URL before starting download to discover file metadata.
pub async fn probe_url(client: &Client, raw_url: &str) -> Result<ProbeResult, String> {
    let url = if !raw_url.starts_with("http://") && !raw_url.starts_with("https://") && !raw_url.starts_with("ftp://") {
        format!("https://{}", raw_url)
    } else {
        raw_url.to_string()
    };

    let mut content_length = 0u64;
    let mut supports_range = false;
    let mut etag = None;
    let mut last_modified = None;
    let mut suggested_filename = None;
    let mut content_type = None;

    // Try HEAD first with timeout
    let head_fut = client
        .head(&url)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .send();

    if let Ok(Ok(resp)) = tokio::time::timeout(Duration::from_secs(5), head_fut).await {
        let final_url = resp.url().to_string();
        if resp.status().is_success() || resp.status().as_u16() == 405 {
            let headers = resp.headers();
            content_length = headers
                .get("content-length")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);

            supports_range = headers
                .get("accept-ranges")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.contains("bytes"))
                .unwrap_or(false);

            etag = headers
                .get("etag")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());

            last_modified = headers
                .get("last-modified")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());

            content_type = headers
                .get("content-type")
                .and_then(|v| v.to_str().ok())
                .map(|v| v.to_string());

            suggested_filename = headers
                .get("content-disposition")
                .and_then(|v| v.to_str().ok())
                .and_then(|v| extract_filename_from_disposition(v))
                .or_else(|| extract_filename_from_url(&final_url))
                .or_else(|| extract_filename_from_url(&url));
        }
    }

    // If HEAD didn't give Content-Length, try a Range GET probe for bytes 0-0
    if content_length == 0 {
        let get_fut = client
            .get(&url)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("Range", "bytes=0-0")
            .send();

        if let Ok(Ok(range_resp)) = tokio::time::timeout(Duration::from_secs(6), get_fut).await {
            let final_url = range_resp.url().to_string();
            let headers = range_resp.headers();
            if range_resp.status().as_u16() == 206 {
                supports_range = true;
                content_length = headers
                    .get("content-range")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| {
                        v.split('/').last().and_then(|s| s.parse::<u64>().ok())
                    })
                    .unwrap_or(0);
            } else if range_resp.status().is_success() {
                content_length = headers
                    .get("content-length")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| v.parse::<u64>().ok())
                    .unwrap_or(0);
            }

            if suggested_filename.is_none() {
                suggested_filename = headers
                    .get("content-disposition")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|v| extract_filename_from_disposition(v))
                    .or_else(|| extract_filename_from_url(&final_url))
                    .or_else(|| extract_filename_from_url(&url));
            }
        }
    }

    if suggested_filename.is_none() {
        suggested_filename = extract_filename_from_url(&url);
    }

    Ok(ProbeResult {
        content_length,
        supports_range,
        etag,
        last_modified,
        suggested_filename,
        content_type,
    })
}

/// Validate server consistency before resuming a download.
///
/// Compares the stored ETag and Last-Modified from the original probe
/// with the current server response. Returns `true` if the file is unchanged
/// and safe to resume, `false` if the file has been modified on the server.
pub fn validate_consistency(
    stored: &DownloadMetadata,
    probe: &ProbeResult,
) -> bool {
    // Content-Length change is an immediate mismatch
    if probe.content_length != stored.total_bytes && probe.content_length > 0 {
        return false;
    }

    // Check ETag if both sides have one
    if let (Some(stored_etag), Some(server_etag)) = (&stored.etag, &probe.etag) {
        if stored_etag != server_etag {
            return false;
        }
    }

    // Check Last-Modified if both sides have one
    if let (Some(stored_lm), Some(server_lm)) = (&stored.last_modified, &probe.last_modified) {
        if stored_lm != server_lm {
            return false;
        }
    }

    true
}

/// Download a single chunk with exponential backoff retries.
///
/// On each attempt, issues a Range GET for `[current_byte, end_byte]`,
/// streams the response body, writes data at the correct file offset,
/// and reports progress through the event channel.
///
/// Retry policy: up to MAX_CHUNK_RETRIES attempts with exponential backoff
/// (500ms * 2^attempt ± random jitter).
pub async fn download_chunk_with_retry(
    client: Client,
    url: String,
    chunk: ChunkInfo,
    file_path: PathBuf,
    throttler: BandwidthThrottler,
    event_tx: mpsc::Sender<WorkerEvent>,
    cancel_rx: watch::Receiver<bool>,
    custom_headers: Option<Vec<(String, String)>>,
) {
    let mut current_byte = chunk.current_byte;
    let end_byte = chunk.end_byte;
    let chunk_index = chunk.index;
    let mut retry_count = chunk.retry_count;

    loop {
        if *cancel_rx.borrow() {
            return; // Cancelled/paused
        }

        if current_byte > end_byte {
            let _ = event_tx
                .send(WorkerEvent::ChunkDone { chunk_index })
                .await;
            return;
        }

        if *cancel_rx.borrow() {
            return;
        }

        match download_chunk_stream(
            &client,
            &url,
            chunk_index,
            current_byte,
            end_byte,
            &file_path,
            &throttler,
            &event_tx,
            &cancel_rx,
            &custom_headers,
        )
        .await
        {
            Ok(final_offset) => {
                if *cancel_rx.borrow() {
                    return;
                }
                current_byte = final_offset;
                if current_byte > end_byte || end_byte == 0 || current_byte >= chunk.end_byte {
                    let _ = event_tx
                        .send(WorkerEvent::ChunkDone { chunk_index })
                        .await;
                    return;
                }
            }
            Err(err) => {
                if *cancel_rx.borrow() {
                    return;
                }
                retry_count += 1;
                let will_retry = retry_count <= MAX_CHUNK_RETRIES;

                let _ = event_tx
                    .send(WorkerEvent::ChunkError {
                        chunk_index,
                        error: err.clone(),
                        will_retry,
                    })
                    .await;

                if !will_retry {
                    eprintln!(
                        "Chunk {} exhausted {} retries: {}",
                        chunk_index, MAX_CHUNK_RETRIES, err
                    );
                    return;
                }

                // Exponential backoff with jitter
                let base_ms = RETRY_BASE_DELAY_MS * 2u64.pow(retry_count as u32 - 1);
                let jitter_ms = (rand::random::<u64>() % (base_ms / 2 + 1)) as i64
                    - (base_ms as i64 / 4);
                let delay_ms = (base_ms as i64 + jitter_ms).max(100) as u64;

                eprintln!(
                    "Chunk {} retry {}/{} in {}ms: {}",
                    chunk_index, retry_count, MAX_CHUNK_RETRIES, delay_ms, err
                );

                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }
    }
}

/// Stream a chunk range from the server, writing data at correct offsets.
///
/// Returns the byte offset after the last successful write.
async fn download_chunk_stream(
    client: &Client,
    url: &str,
    chunk_index: usize,
    start_byte: u64,
    end_byte: u64,
    file_path: &Path,
    throttler: &BandwidthThrottler,
    event_tx: &mpsc::Sender<WorkerEvent>,
    cancel_rx: &watch::Receiver<bool>,
    custom_headers: &Option<Vec<(String, String)>>,
) -> Result<u64, String> {
    let mut req = client
        .get(url)
        .header("User-Agent", DEFAULT_USER_AGENT)
        .header("Accept", "*/*");

    if end_byte > start_byte {
        let range_header = format!("bytes={}-{}", start_byte, end_byte);
        req = req.header("Range", range_header);
    } else if start_byte > 0 {
        let range_header = format!("bytes={}-", start_byte);
        req = req.header("Range", range_header);
    }

    // Add custom headers (Authorization, Cookie, etc.)
    if let Some(headers) = custom_headers {
        for (key, value) in headers {
            req = req.header(key.as_str(), value.as_str());
        }
    }

    let resp = req
        .send()
        .await
        .map_err(|e| format!("Connection failed for chunk {}: {}", chunk_index, e))?;

    let status = resp.status();
    if status.as_u16() != 206 && status.as_u16() != 200 {
        return Err(format!(
            "Server returned {} for chunk {} (start_byte: {}, end_byte: {})",
            status, chunk_index, start_byte, end_byte
        ));
    }

    // If server returned 200 OK (full body from byte 0) instead of 206 Partial Content:
    // Only chunk 0 can write from offset 0; secondary chunks exit cleanly to avoid corruption.
    let mut offset = if status.as_u16() == 200 {
        if chunk_index > 0 {
            return Ok(end_byte + 1);
        }
        0
    } else {
        start_byte
    };

    // Open file for this chunk worker with shared read/write access
    let mut file = storage::open_shared_file(file_path)
        .map_err(|e| format!("Failed to open file for chunk {}: {}", chunk_index, e))?;

    let mut stream = resp.bytes_stream();

    while let Some(result) = stream.next().await {
        if *cancel_rx.borrow() {
            let _ = file.flush().await;
            return Ok(offset); // Paused — return current position
        }

        let bytes = result.map_err(|e| {
            format!("Stream error on chunk {} at offset {}: {}", chunk_index, offset, e)
        })?;

        let len = bytes.len() as u64;

        // Apply bandwidth throttling
        throttler.acquire(bytes.len()).await;

        // Write at exact offset
        storage::write_at_offset(&mut file, offset, &bytes).await?;

        offset += len;

        // Report progress
        let _ = event_tx
            .send(WorkerEvent::Progress {
                chunk_index,
                bytes_written: len,
                new_offset: offset,
            })
            .await;
    }

    let _ = file.flush().await;
    Ok(offset)
}

/// Extract filename from Content-Disposition header supporting RFC 6266 / RFC 5987.
pub fn extract_filename_from_disposition(header: &str) -> Option<String> {
    // 1. Try RFC 5987 / RFC 6266 filename*=charset'lang'encoded-value (Highest Priority)
    if let Some(pos) = header.find("filename*=") {
        let rest = &header[pos + 10..];
        let val_part = rest.split(';').next().unwrap_or(rest).trim().trim_matches('"');
        if let Some(quote_pos) = val_part.rfind("''") {
            let encoded_name = &val_part[quote_pos + 2..];
            let decoded = urlencoding_decode(encoded_name);
            let clean = clean_filename(&decoded);
            if !clean.is_empty() {
                return Some(clean);
            }
        } else {
            let decoded = urlencoding_decode(val_part);
            let clean = clean_filename(&decoded);
            if !clean.is_empty() {
                return Some(clean);
            }
        }
    }

    // 2. Standard filename="name.zip" or filename=name.zip
    if let Some(pos) = header.find("filename=") {
        let rest = &header[pos + 9..];
        let trimmed = rest.trim();
        let name = if trimmed.starts_with('"') {
            // Quoted string: take until matching closing quote
            if let Some(end_quote) = trimmed[1..].find('"') {
                &trimmed[1..=end_quote]
            } else {
                trimmed.trim_matches('"')
            }
        } else {
            // Unquoted string: take until semicolon
            trimmed.split(';').next().unwrap_or(trimmed).trim()
        };

        let clean = clean_filename(name);
        if !clean.is_empty() {
            return Some(clean);
        }
    }

    None
}

/// Extract clean filename from the URL path.
pub fn extract_filename_from_url(url: &str) -> Option<String> {
    let clean_path = url.split('?').next()?.split('#').next()?;
    let segment = clean_path.split('/').last().filter(|s| !s.is_empty())?;
    let decoded = urlencoding_decode(segment);
    let clean = clean_filename(&decoded);
    if clean.contains('.') && !clean.starts_with('.') && clean.len() > 1 {
        Some(clean)
    } else {
        None
    }
}

/// Clean filename by removing invalid filesystem characters and path traversal tokens.
pub fn clean_filename(s: &str) -> String {
    let unquoted = s.trim().trim_matches('"').trim_matches('\'');
    let decoded = urlencoding_decode(unquoted);
    let sanitized: String = decoded
        .chars()
        .map(|c| match c {
            '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|' | '\0'..='\x1f' => '_',
            _ => c,
        })
        .collect();

    let cleaned = sanitized
        .replace("..", "_")
        .trim_matches(|c: char| c == '.' || c == ' ' || c == '_')
        .to_string();

    if cleaned.is_empty() {
        "download.dat".to_string()
    } else {
        cleaned
    }
}

/// Simple percent-decoding for URL filenames.
pub fn urlencoding_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut chars = s.as_bytes().iter().cloned();
    while let Some(b) = chars.next() {
        if b == b'%' {
            let h1 = chars.next();
            let h2 = chars.next();
            if let (Some(h1), Some(h2)) = (h1, h2) {
                let hex_str = [h1, h2];
                if let Ok(hex_utf8) = std::str::from_utf8(&hex_str) {
                    if let Ok(byte_val) = u8::from_str_radix(hex_utf8, 16) {
                        bytes.push(byte_val);
                        continue;
                    }
                }
                bytes.push(b'%');
                bytes.push(h1);
                bytes.push(h2);
            } else {
                bytes.push(b'%');
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).unwrap_or_else(|_| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_filename_from_disposition() {
        assert_eq!(
            extract_filename_from_disposition("attachment; filename=\"ubuntu-22.04.iso\""),
            Some("ubuntu-22.04.iso".to_string())
        );
        assert_eq!(
            extract_filename_from_disposition("inline; filename=report.pdf"),
            Some("report.pdf".to_string())
        );
    }

    #[test]
    fn test_extract_filename_from_url() {
        assert_eq!(
            extract_filename_from_url("https://example.com/downloads/file.zip?key=abc"),
            Some("file.zip".to_string())
        );
        assert_eq!(
            extract_filename_from_url("https://example.com/downloads/"),
            None
        );
    }

    #[test]
    fn test_validate_consistency_matching() {
        let meta = DownloadMetadata {
            id: "test".into(),
            url: "https://example.com/f.zip".into(),
            destination: "/tmp/f.zip".into(),
            filename: "f.zip".into(),
            total_bytes: 1000,
            status: TaskStatus::Paused,
            etag: Some("\"abc\"".into()),
            last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".into()),
            supports_range: true,
            num_connections: 8,
            priority: 0,
            category: "archive".into(),
            chunks: vec![],
            created_at: 0,
            updated_at: 0,
        };

        let probe = ProbeResult {
            content_length: 1000,
            supports_range: true,
            etag: Some("\"abc\"".into()),
            last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".into()),
            suggested_filename: None,
            content_type: None,
        };

        assert!(validate_consistency(&meta, &probe));
    }

    #[test]
    fn test_validate_consistency_etag_mismatch() {
        let meta = DownloadMetadata {
            id: "test".into(),
            url: "https://example.com/f.zip".into(),
            destination: "/tmp/f.zip".into(),
            filename: "f.zip".into(),
            total_bytes: 1000,
            status: TaskStatus::Paused,
            etag: Some("\"abc\"".into()),
            last_modified: None,
            supports_range: true,
            num_connections: 8,
            priority: 0,
            category: "archive".into(),
            chunks: vec![],
            created_at: 0,
            updated_at: 0,
        };

        let probe = ProbeResult {
            content_length: 1000,
            supports_range: true,
            etag: Some("\"xyz\"".into()),  // Changed!
            last_modified: None,
            suggested_filename: None,
            content_type: None,
        };

        assert!(!validate_consistency(&meta, &probe));
    }

    #[test]
    fn test_validate_consistency_size_change() {
        let meta = DownloadMetadata {
            id: "test".into(),
            url: "https://example.com/f.zip".into(),
            destination: "/tmp/f.zip".into(),
            filename: "f.zip".into(),
            total_bytes: 1000,
            status: TaskStatus::Paused,
            etag: None,
            last_modified: None,
            supports_range: true,
            num_connections: 8,
            priority: 0,
            category: "archive".into(),
            chunks: vec![],
            created_at: 0,
            updated_at: 0,
        };

        let probe = ProbeResult {
            content_length: 2000,  // Size changed!
            supports_range: true,
            etag: None,
            last_modified: None,
            suggested_filename: None,
            content_type: None,
        };

        assert!(!validate_consistency(&meta, &probe));
    }

    #[tokio::test]
    async fn test_real_download_chunk() {
        let client = Client::builder().build().unwrap();
        let probe = probe_url(&client, "https://proof.ovh.net/files/10Mb.dat").await.unwrap();
        assert_eq!(probe.content_length, 10485760);
        assert!(probe.supports_range);
    }
}
