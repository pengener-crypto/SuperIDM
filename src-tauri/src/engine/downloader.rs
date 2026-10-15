use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use reqwest::Client;
use tokio::io::AsyncWriteExt;
use tokio::sync::{mpsc, watch};

use super::speed::SpeedMeter;
use super::storage;
use super::throttler::BandwidthThrottler;
use super::types::*;

/// Thread-safe map tracking dynamic end limits for active chunks during work stealing.
pub type ChunkLimitMap = Arc<parking_lot::RwLock<HashMap<usize, Arc<AtomicU64>>>>;

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
            .tcp_nodelay(true)
            .pool_max_idle_per_host(64)
            .pool_idle_timeout(Some(Duration::from_secs(120)))
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
    chunk_limits: Option<ChunkLimitMap>,
) {
    let mut current_byte = chunk.current_byte;
    let end_byte = chunk.end_byte;
    let chunk_index = chunk.index;
    let mut retry_count = chunk.retry_count;

    let chunk_limit = if let Some(ref limits) = chunk_limits {
        let mut map = limits.write();
        map.entry(chunk_index)
            .or_insert_with(|| Arc::new(AtomicU64::new(end_byte)))
            .clone()
    } else {
        Arc::new(AtomicU64::new(end_byte))
    };

    loop {
        if *cancel_rx.borrow() {
            return; // Cancelled/paused
        }

        let cur_limit = chunk_limit.load(Ordering::SeqCst);
        let target_end = if cur_limit > 0 { cur_limit } else { end_byte };
        // Chunk range is inclusive: done only once every byte up to target_end is written.
        if current_byte > target_end {
            let _ = event_tx
                .send(WorkerEvent::ChunkDone { chunk_index })
                .await;
            return;
        }

        let attempt_start = current_byte;
        let result = download_chunk_stream(
            &client,
            &url,
            chunk_index,
            current_byte,
            target_end,
            &file_path,
            &throttler,
            &event_tx,
            &cancel_rx,
            &custom_headers,
            &chunk_limit,
        )
        .await;

        if *cancel_rx.borrow() {
            return;
        }

        let err = match result {
            Ok(outcome) => {
                current_byte = outcome.next_offset;
                let cur_limit = chunk_limit.load(Ordering::SeqCst);
                let target_end = if cur_limit > 0 { cur_limit } else { end_byte };
                if outcome.full_body_eof || current_byte > target_end {
                    let _ = event_tx
                        .send(WorkerEvent::ChunkDone { chunk_index })
                        .await;
                    return;
                }
                if current_byte > attempt_start {
                    // Server closed early but we made progress: reconnect from the new offset.
                    continue;
                }
                format!(
                    "Chunk {} stream ended early with no progress at offset {}",
                    chunk_index, current_byte
                )
            }
            Err(e) => e,
        };

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

/// Result of one streaming attempt for a chunk.
struct StreamOutcome {
    /// First byte offset NOT yet written (exclusive end of written data).
    next_offset: u64,
    /// Server sent the whole body (200 OK, no range) and it ended cleanly.
    full_body_eof: bool,
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
    chunk_limit: &Arc<AtomicU64>,
) -> Result<StreamOutcome, String> {
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

    // If server returned 200 OK (full body from byte 0) instead of 206 Partial Content,
    // only a chunk whose data starts at byte 0 can use it. Any other chunk must retry —
    // reporting it as "done" without writing would leave a hole of zeros in the file.
    let is_full_body = status.as_u16() == 200;
    let mut offset = if is_full_body {
        if chunk_index > 0 {
            return Err(format!(
                "Server ignored Range for chunk {} (got 200 instead of 206)",
                chunk_index
            ));
        }
        0
    } else {
        // Verify the server actually sent the range we asked for.
        let range_start = resp
            .headers()
            .get("content-range")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.trim().strip_prefix("bytes "))
            .and_then(|v| v.split('-').next())
            .and_then(|v| v.trim().parse::<u64>().ok());
        if let Some(rs) = range_start {
            if rs != start_byte {
                return Err(format!(
                    "Content-Range mismatch for chunk {}: asked {}, got {}",
                    chunk_index, start_byte, rs
                ));
            }
        }
        start_byte
    };

    // Open file for this chunk worker with shared read/write access
    let mut file = storage::open_shared_file(file_path)
        .map_err(|e| format!("Failed to open file for chunk {}: {}", chunk_index, e))?;

    let mut stream = resp.bytes_stream();
    let mut write_buf: Vec<u8> = Vec::with_capacity(256 * 1024);
    let mut buf_start_offset = offset;
    let mut pending_bytes_reported: u64 = 0;
    let mut last_report_time = std::time::Instant::now();

    while let Some(result) = stream.next().await {
        if *cancel_rx.borrow() {
            if !write_buf.is_empty() {
                storage::write_at_offset(&mut file, buf_start_offset, &write_buf).await?;
                write_buf.clear();
            }
            if pending_bytes_reported > 0 {
                let _ = event_tx
                    .send(WorkerEvent::Progress {
                        chunk_index,
                        bytes_written: pending_bytes_reported,
                        new_offset: offset,
                    })
                    .await;
            }
            let _ = file.flush().await;
            return Ok(StreamOutcome { next_offset: offset, full_body_eof: false }); // Paused
        }

        let bytes = result.map_err(|e| {
            format!("Stream error on chunk {} at offset {}: {}", chunk_index, offset, e)
        })?;

        let len = bytes.len() as u64;

        // Apply bandwidth throttling
        throttler.acquire(bytes.len()).await;

        write_buf.extend_from_slice(&bytes);
        offset += len;
        pending_bytes_reported += len;

        // Check if dynamic work-stealing shrunk this chunk's end boundary.
        // cur_limit is an INCLUSIVE end byte; `offset` is EXCLUSIVE.
        let cur_limit = chunk_limit.load(Ordering::SeqCst);
        let stop_at = cur_limit.saturating_add(1); // exclusive
        if cur_limit > 0 && offset > stop_at {
            // Keep every byte with absolute position < stop_at.
            let keep_len = stop_at.saturating_sub(buf_start_offset).min(write_buf.len() as u64) as usize;
            let dropped = (write_buf.len() - keep_len) as u64;
            write_buf.truncate(keep_len);
            pending_bytes_reported = pending_bytes_reported.saturating_sub(dropped);
            if !write_buf.is_empty() {
                storage::write_at_offset(&mut file, buf_start_offset, &write_buf).await?;
                write_buf.clear();
            }
            if pending_bytes_reported > 0 {
                let _ = event_tx
                    .send(WorkerEvent::Progress {
                        chunk_index,
                        bytes_written: pending_bytes_reported,
                        new_offset: stop_at,
                    })
                    .await;
            }
            let _ = file.flush().await;
            return Ok(StreamOutcome { next_offset: stop_at, full_body_eof: false });
        }

        // Flush write buffer if reached 256 KB
        if write_buf.len() >= 256 * 1024 {
            storage::write_at_offset(&mut file, buf_start_offset, &write_buf).await?;
            write_buf.clear();
            buf_start_offset = offset;
        }

        // Batch progress reporting: every >= 64 KB or every >= 50ms
        if pending_bytes_reported >= 64 * 1024 || last_report_time.elapsed().as_millis() >= 50 {
            let _ = event_tx
                .send(WorkerEvent::Progress {
                    chunk_index,
                    bytes_written: pending_bytes_reported,
                    new_offset: offset,
                })
                .await;
            pending_bytes_reported = 0;
            last_report_time = std::time::Instant::now();
        }
    }

    // Flush any remaining buffered bytes
    if !write_buf.is_empty() {
        storage::write_at_offset(&mut file, buf_start_offset, &write_buf).await?;
        write_buf.clear();
    }
    if pending_bytes_reported > 0 {
        let _ = event_tx
            .send(WorkerEvent::Progress {
                chunk_index,
                bytes_written: pending_bytes_reported,
                new_offset: offset,
            })
            .await;
    }

    let _ = file.flush().await;
    Ok(StreamOutcome { next_offset: offset, full_body_eof: is_full_body })
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
    let mut unquoted = s.trim().trim_matches('"').trim_matches('\'');

    // Strip notification counter prefixes like "(3) ", "[1] "
    if let Some(rest) = unquoted.strip_prefix('(') {
        if let Some(close_idx) = rest.find(')') {
            if rest[..close_idx].chars().all(|c| c.is_ascii_digit()) {
                unquoted = rest[close_idx + 1..].trim_start();
            }
        }
    } else if let Some(rest) = unquoted.strip_prefix('[') {
        if let Some(close_idx) = rest.find(']') {
            if rest[..close_idx].chars().all(|c| c.is_ascii_digit()) {
                unquoted = rest[close_idx + 1..].trim_start();
            }
        }
    }

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

    /// Segmented download + mid-stream work-steal must be byte-identical to a plain GET.
    #[tokio::test]
    async fn test_work_steal_integrity() {
        use crate::engine::chunker;
        let url = "https://proof.ovh.net/files/10Mb.dat".to_string();
        let client = Client::builder().build().unwrap();
        let expected = client.get(&url).send().await.unwrap().bytes().await.unwrap();
        let total = expected.len() as u64;

        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("steal.bin");
        storage::preallocate_file(&path, total).await.unwrap();

        let mut chunks = chunker::calculate_chunks(total, 2);
        // Throttle so chunks are still streaming when the steal happens.
        let throttler = BandwidthThrottler::new(3 * 1024 * 1024);
        let (tx, mut rx) = mpsc::channel::<WorkerEvent>(4096);
        let (_cancel_tx, cancel_rx) = watch::channel(false);
        let limits: ChunkLimitMap = Arc::new(parking_lot::RwLock::new(HashMap::new()));

        let spawn = |c: ChunkInfo| {
            let (cl, u, p, th, t, cr, lm) = (client.clone(), url.clone(), path.clone(),
                throttler.clone(), tx.clone(), cancel_rx.clone(), Some(limits.clone()));
            tokio::spawn(async move {
                download_chunk_with_retry(cl, u, c, p, th, t, cr, None, lm).await;
            });
        };
        for c in chunks.clone() { spawn(c); }

        let mut stolen = false;
        let mut done = 0usize;
        while let Some(ev) = rx.recv().await {
            match ev {
                WorkerEvent::Progress { chunk_index, new_offset, .. } => {
                    if chunk_index < chunks.len() { chunks[chunk_index].current_byte = new_offset; }
                    // Steal once the donor has made real progress (mirrors run_coordinator).
                    if !stolen && chunks[1].current_byte > chunks[1].start_byte + 512 * 1024 {
                        if let Some((donor, nc)) = chunker::steal_work(&mut chunks) {
                            limits.read().get(&donor).unwrap()
                                .store(chunks[donor].end_byte, Ordering::SeqCst);
                            limits.write().insert(nc.index, Arc::new(AtomicU64::new(nc.end_byte)));
                            chunks.push(nc.clone());
                            spawn(nc);
                            stolen = true;
                        }
                    }
                }
                WorkerEvent::ChunkDone { .. } => {
                    done += 1;
                    if stolen && done == chunks.len() { break; }
                }
                WorkerEvent::ChunkError { error, will_retry, .. } => {
                    assert!(will_retry, "chunk failed permanently: {}", error);
                }
            }
        }
        assert!(stolen, "work steal never triggered");

        let actual = std::fs::read(&path).unwrap();
        assert_eq!(actual.len(), expected.len());
        let first_diff = actual.iter().zip(expected.iter()).position(|(a, b)| a != b);
        assert_eq!(first_diff, None, "file differs from reference at byte {:?}", first_diff);
    }
}
