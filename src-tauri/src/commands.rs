use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, Manager, State};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::engine::chunker;
use crate::engine::downloader::{self, DownloadManager, TaskHandle, WorkerEvent};
use crate::engine::speed::SpeedMeter;
use crate::engine::storage;
use crate::engine::throttler::BandwidthThrottler;
use crate::engine::types::*;

/// Shared application state managed by Tauri.
pub struct AppState {
    pub manager: Arc<RwLock<DownloadManager>>,
}

/// Get the system's default download directory.
#[tauri::command]
pub fn get_default_save_path(state: State<'_, AppState>) -> String {
    state
        .manager
        .read()
        .default_download_dir
        .to_string_lossy()
        .to_string()
}

/// Start a new download task.
#[tauri::command]
pub async fn start_download(
    state: State<'_, AppState>,
    app: AppHandle,
    request: serde_json::Value,
) -> Result<String, String> {
    // Support nested `{ "request": { ... } }` or flat `{ "url": ... }`
    let req_obj = if let Some(inner) = request.get("request") {
        inner
    } else {
        &request
    };

    // Fast path using typed NewDownloadRequest, with flexible fallback for string priority / map headers
    let typed_req = serde_json::from_value::<NewDownloadRequest>(req_obj.clone()).ok();

    let raw_url = typed_req
        .as_ref()
        .map(|r| r.url.as_str())
        .or_else(|| req_obj.get("url").and_then(|v| v.as_str()))
        .ok_or_else(|| "Missing 'url' parameter in download request".to_string())?;
    let url = crate::engine::extractor::MediaExtractor::clean_target_url(raw_url);

    let destination = typed_req
        .as_ref()
        .map(|r| r.destination.clone())
        .unwrap_or_else(|| {
            req_obj
                .get("destination")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        });

    let filename = typed_req
        .as_ref()
        .and_then(|r| r.filename.clone())
        .or_else(|| {
            req_obj
                .get("filename")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let format_id = typed_req
        .as_ref()
        .and_then(|r| r.format.clone())
        .or_else(|| {
            req_obj
                .get("format")
                .or_else(|| req_obj.get("format_id"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });

    let num_connections = typed_req
        .as_ref()
        .and_then(|r| r.num_connections)
        .or_else(|| {
            req_obj
                .get("numConnections")
                .or_else(|| req_obj.get("num_connections"))
                .and_then(|v| {
                    v.as_u64()
                        .map(|n| n as usize)
                        .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                })
        })
        .unwrap_or(DEFAULT_CONNECTIONS);

    let priority = typed_req
        .as_ref()
        .and_then(|r| r.priority)
        .or_else(|| {
            req_obj.get("priority").and_then(|v| {
                v.as_u64()
                    .map(|n| n as u8)
                    .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
            })
        })
        .unwrap_or(0);

    let category = typed_req
        .as_ref()
        .and_then(|r| r.category.clone())
        .unwrap_or_else(|| {
            req_obj
                .get("category")
                .and_then(|v| v.as_str())
                .unwrap_or("General")
                .to_string()
        });

    let headers: Option<Vec<(String, String)>> = typed_req
        .and_then(|r| r.headers)
        .or_else(|| {
            req_obj.get("headers").and_then(|h| {
                if let Some(arr) = h.as_array() {
                    let mut list = Vec::new();
                    for item in arr {
                        if let Some(pair) = item.as_array() {
                            if pair.len() >= 2 {
                                if let (Some(k), Some(v)) = (pair[0].as_str(), pair[1].as_str()) {
                                    list.push((k.to_string(), v.to_string()));
                                }
                            }
                        }
                    }
                    Some(list)
                } else if let Some(map) = h.as_object() {
                    let mut list = Vec::new();
                    for (k, v) in map {
                        if let Some(val_str) = v.as_str() {
                            list.push((k.clone(), val_str.to_string()));
                        }
                    }
                    Some(list)
                } else {
                    None
                }
            })
        });

    let task_id = Uuid::new_v4().to_string();

    let default_dir = state.manager.read().default_download_dir.clone();
    let dest_dir = if destination.is_empty() || destination.contains("Users\\User") {
        default_dir
    } else {
        let p = PathBuf::from(&destination);
        if p.exists() || p.parent().map(|par| par.exists()).unwrap_or(false) {
            p
        } else {
            default_dir
        }
    };
    let _ = tokio::fs::create_dir_all(&dest_dir).await;

    // Route through yt-dlp media extraction engine if:
    // 1. Neither the filename nor URL looks like a direct file download (.zip, .exe, .rar, etc.)
    // AND
    // 2. Either it's a known media platform URL, OR format_id was explicitly requested
    let fn_is_direct = filename
        .as_deref()
        .map(crate::engine::extractor::MediaExtractor::is_likely_direct_file)
        .unwrap_or(false);
    let url_is_direct = crate::engine::extractor::MediaExtractor::is_likely_direct_file(&url);
    let is_direct_file = fn_is_direct || url_is_direct;

    let use_media_engine = !is_direct_file
        && (crate::engine::extractor::MediaExtractor::is_platform_url(&url) || format_id.is_some());
    if use_media_engine {
        let (cancel_tx, cancel_rx) = watch::channel(false);
        let (prog_tx, mut prog_rx) = mpsc::channel::<(f64, f64, u64, u64, f64, bool, Option<String>)>(128);

        let initial_name = filename.unwrap_or_else(|| "Video_Stream.mp4".to_string());
        let file_path = dest_dir.join(&initial_name);
        let now = chrono::Utc::now().timestamp_millis();

        let is_audio_cat = format_id
            .as_deref()
            .map(|f| {
                let fl = f.to_lowercase();
                fl.contains("audio") || fl == "mp3"
            })
            .unwrap_or(false);

        let metadata = DownloadMetadata {
            id: task_id.clone(),
            url: url.clone(),
            destination: file_path.to_string_lossy().to_string(),
            filename: initial_name,
            total_bytes: 0,
            status: TaskStatus::Downloading,
            etag: None,
            last_modified: None,
            supports_range: true,
            num_connections: num_connections.clamp(1, 32),
            priority,
            category: if is_audio_cat { "Music".to_string() } else { "Videos".to_string() },
            format_id: format_id.clone(),
            chunks: Vec::new(),
            created_at: now,
            updated_at: now,
        };

        {
            let mut mgr = state.manager.write();
            mgr.tasks.insert(
                task_id.clone(),
                TaskHandle {
                    metadata,
                    speed_meter: SpeedMeter::new(),
                    cancel_token: cancel_tx,
                },
            );
        }
        {
            let all_tasks: Vec<_> = state.manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
            let _ = storage::save_tasks_db(&all_tasks).await;
        }

        let client_c = state.manager.read().http_client.clone();
        let url_c = url.clone();
        let dest_dir_ext = dest_dir.clone();
        let dest_dir_watch = dest_dir.clone();
        let cancel_c = cancel_rx.clone();
        let format_id_c = format_id.clone();
        let media_headers_c: Vec<(String, String)> = headers.clone().unwrap_or_default();
        let app_err = app.clone();
        let task_id_err = task_id.clone();

        // First-run component setup can take a while on slow connections; tell the user instead
        // of showing a silent "0% · 0 B/s" row.
        {
            use crate::engine::extractor::MediaExtractor as MX;
            let mut missing: Vec<&str> = Vec::new();
            if MX::find_ytdlp_binary().is_none() { missing.push("yt-dlp"); }
            if MX::find_ffmpeg_binary().is_none() { missing.push("FFmpeg"); }
            if MX::is_youtube_url(&url) && MX::js_runtime_arg().is_none() { missing.push("JS Runtime"); }
            if !missing.is_empty() {
                let _ = app.emit("engine_notice", serde_json::json!({
                    "id": task_id,
                    "stage": "preparing_components",
                    "code": "preparing_components",
                    "components": missing,
                }));
            }
        }

        // Spawn platform stream extractor
        let mgr_err_arc = state.manager.clone();
        tokio::spawn(async move {
            let res = crate::engine::extractor::MediaExtractor::download_media_stream(
                &client_c,
                &url_c,
                &dest_dir_ext,
                format_id_c.as_deref(),
                &media_headers_c,
                prog_tx,
                cancel_c,
            ).await;
            if let Err(e) = res {
                let was_user_paused = {
                    let mgr = mgr_err_arc.read();
                    mgr.tasks
                        .get(&task_id_err)
                        .map(|h| h.metadata.status == TaskStatus::Paused)
                        .unwrap_or(true)
                };
                if !was_user_paused && !e.to_lowercase().contains("cancelled") {
                    eprintln!("[MediaExtractor Error] {}", e);
                    let _ = app_err.emit("download_error", serde_json::json!({
                        "id": task_id_err,
                        "message": e,
                    }));
                }
            }
        });

        // Watch progress
        let app_handle = app.clone();
        let task_id_c = task_id.clone();
        let mgr_arc = state.manager.clone();

        tokio::spawn(async move {
            let mut completed = false;
            let mut prev_downloaded = 0u64;
            while let Some((progress, speed, downloaded, total, eta_secs, is_comp, final_name_opt)) = prog_rx.recv().await {
                let raw_total = if total > 0 { total } else if downloaded > 0 { downloaded } else { 0 };
                let eff_downloaded = downloaded;
                let eff_total = raw_total.max(eff_downloaded);
                {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        if eff_downloaded > prev_downloaded {
                            h.speed_meter.record(eff_downloaded - prev_downloaded);
                            prev_downloaded = eff_downloaded;
                        }
                        h.metadata.total_bytes = eff_total;
                        if let Some(ref fname) = final_name_opt {
                            h.metadata.filename = fname.clone();
                            let new_dest = dest_dir_watch.join(fname);
                            h.metadata.destination = new_dest.to_string_lossy().to_string();
                        }
                        let prog_bp = (progress.clamp(0.0, 1.0) * 10000.0) as usize;
                        if h.metadata.chunks.is_empty() {
                            h.metadata.chunks.push(ChunkInfo {
                                index: 0,
                                start_byte: 0,
                                end_byte: eff_total,
                                current_byte: eff_downloaded,
                                completed: is_comp,
                                retry_count: prog_bp,
                            });
                        } else {
                            h.metadata.chunks[0].start_byte = 0;
                            h.metadata.chunks[0].current_byte = eff_downloaded;
                            h.metadata.chunks[0].end_byte = eff_total;
                            h.metadata.chunks[0].completed = is_comp;
                            h.metadata.chunks[0].retry_count = prog_bp;
                        }
                        if is_comp {
                            h.metadata.status = TaskStatus::Completed;
                            h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
                            h.speed_meter.reset();
                            completed = true;
                        }
                    }
                }

                let payload = ProgressPayload {
                    id: task_id_c.clone(),
                    status: if is_comp { TaskStatus::Completed } else { TaskStatus::Downloading },
                    downloaded_bytes: eff_downloaded,
                    total_bytes: eff_total,
                    progress,
                    speed: if is_comp { 0.0 } else { speed },
                    chunks: vec![],
                    eta_secs: if is_comp { 0.0 } else { eta_secs },
                };

                let _ = app_handle.emit("download_progress", &payload);

                if is_comp {
                    let all_tasks: Vec<_> = mgr_arc.read().tasks.values().map(|h| h.metadata.clone()).collect();
                    let _ = storage::save_tasks_db(&all_tasks).await;
                    break;
                }
            }

            if !completed {
                let maybe_fail = {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        if h.metadata.status != TaskStatus::Completed && h.metadata.status != TaskStatus::Paused {
                            h.metadata.status = TaskStatus::Failed;
                            h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
                            let payload = ProgressPayload {
                                id: task_id_c.clone(),
                                status: TaskStatus::Failed,
                                downloaded_bytes: 0,
                                total_bytes: 0,
                                progress: 0.0,
                                speed: 0.0,
                                chunks: vec![],
                                eta_secs: 0.0,
                            };
                            Some(payload)
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(payload) = maybe_fail {
                    let _ = app_handle.emit("download_progress", &payload);
                    let all_tasks: Vec<_> = mgr_arc.read().tasks.values().map(|h| h.metadata.clone()).collect();
                    let _ = storage::save_tasks_db(&all_tasks).await;
                }
            }
        });

        return Ok(task_id);
    }

    // Direct HTTP/HTTPS/FTP Probe & multi-chunk download
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    let probe = downloader::probe_url(&client, &url).await?;

    // Determine filename:
    // 1. If caller provided an explicit descriptive filename (e.g. from browser extension or user), prioritize it!
    let explicit_name = filename.as_ref().filter(|f| {
        let f_trim = f.trim();
        let f_clean = crate::engine::downloader::clean_filename(f_trim);
        let f_lower = f_clean.to_lowercase();
        !f_clean.is_empty() 
            && f_clean != "download" 
            && f_clean != "download.dat"
            && f_clean != "video" 
            && f_clean != "video.mp4"
            && f_lower != "instagram.mp4"
            && f_lower != "youtube.mp4"
            && f_lower != "facebook.mp4"
            && f_lower != "tiktok.mp4"
            && !f_lower.starts_with("download_")
            && !f_lower.starts_with("instagram_video_")
    });

    let raw_filename = if let Some(exp) = explicit_name {
        let safe_name = crate::engine::downloader::clean_filename(exp);
        if !safe_name.contains('.') {
            if let Some(ref ct) = probe.content_type {
                let ext = if ct.contains("video") || ct.contains("mp4") { "mp4" }
                    else if ct.contains("audio") || ct.contains("mpeg") { "mp3" }
                    else if ct.contains("zip") { "zip" }
                    else if ct.contains("pdf") { "pdf" }
                    else { "dat" };
                format!("{}.{}", safe_name, ext)
            } else {
                format!("{}.mp4", safe_name)
            }
        } else {
            safe_name
        }
    } else if let Some(suggested) = probe.suggested_filename {
        if !suggested.is_empty() && suggested != "download" && !suggested.starts_with("download_") {
            suggested
        } else {
            filename.unwrap_or_else(|| format!("download_{}.dat", &task_id[..8]))
        }
    } else {
        filename.unwrap_or_else(|| format!("download_{}.dat", &task_id[..8]))
    };

    // Auto-resolve duplicate filenames (e.g. video (1).mp4) to prevent data overwrite
    let filename = storage::resolve_unique_filename(&dest_dir, &raw_filename);
    let file_path = dest_dir.join(&filename);

    let total_bytes = probe.content_length;

    // Pre-flight Disk Space Check (fail fast before pre-allocation or network usage)
    if probe.content_length > 0 {
        if let Ok(available_space) = storage::get_available_disk_space(&dest_dir) {
            let safety_margin = 10 * 1024 * 1024; // 10MB safety buffer
            if available_space < total_bytes.saturating_add(safety_margin) {
                let req_mb = total_bytes / (1024 * 1024);
                let avail_mb = available_space / (1024 * 1024);
                return Err(format!(
                    "Not enough disk space: {} MB required, but only {} MB available in {}",
                    req_mb,
                    avail_mb,
                    dest_dir.display()
                ));
            }
        }
    }

    let effective_conns = if probe.supports_range && probe.content_length > 0 {
        if url.contains("fbcdn.net") || url.contains("cdninstagram.com") {
            1 // Facebook CDN throttles or breaks on parallel range requests
        } else {
            num_connections.clamp(1, 32)
        }
    } else {
        1
    };

    let chunks = if total_bytes > 0 {
        chunker::calculate_chunks(total_bytes, effective_conns)
    } else {
        vec![ChunkInfo {
            index: 0,
            start_byte: 0,
            end_byte: 0,
            current_byte: 0,
            completed: false,
            retry_count: 0,
        }]
    };

    // Pre-allocate file only when server reported an exact Content-Length
    if total_bytes > 0 {
        let _ = storage::preallocate_file(&file_path, total_bytes).await;
    }

    let now = chrono::Utc::now().timestamp_millis();

    let metadata = DownloadMetadata {
        id: task_id.clone(),
        url: url.clone(),
        destination: file_path.to_string_lossy().to_string(),
        filename: filename.clone(),
        total_bytes,
        status: TaskStatus::Downloading,
        etag: probe.etag.clone(),
        last_modified: probe.last_modified.clone(),
        supports_range: probe.supports_range,
        num_connections: effective_conns,
        priority,
        category,
        format_id: None,
        chunks: chunks.clone(),
        created_at: now,
        updated_at: now,
    };

    // Save initial .part.json
    let _ = storage::save_part_json_atomic(&file_path, &metadata).await;

    // Create cancel token
    let (cancel_tx, cancel_rx) = watch::channel(false);

    // Register task in state
    {
        let mut mgr = state.manager.write();
        mgr.tasks.insert(
            task_id.clone(),
            TaskHandle {
                metadata: metadata.clone(),
                speed_meter: SpeedMeter::new(),
                cancel_token: cancel_tx,
            },
        );
    }

    // Spawn chunk workers with shared event channel and dynamic limit tracking
    let throttler = state.manager.read().throttler.clone();
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>(4096);
    let chunk_limits: downloader::ChunkLimitMap = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    for chunk in &chunks {
        if chunk.completed {
            continue;
        }
        let client_c = client.clone();
        let url_c = url.clone();
        let chunk_c = chunk.clone();
        let file_path_c = file_path.clone();
        let throttler_c = throttler.clone();
        let cancel_rx_c = cancel_rx.clone();
        let headers_c = headers.clone();
        let tx = event_tx.clone();
        let limits_c = Some(chunk_limits.clone());

        tokio::spawn(async move {
            downloader::download_chunk_with_retry(
                client_c,
                url_c,
                chunk_c,
                file_path_c,
                throttler_c,
                tx,
                cancel_rx_c,
                headers_c,
                limits_c,
            )
            .await;
        });
    }

    let coordinator_ctx = CoordinatorContext {
        client: client.clone(),
        throttler: throttler.clone(),
        event_tx: event_tx.clone(),
        cancel_rx: cancel_rx.clone(),
        custom_headers: headers.clone(),
        chunk_limits: chunk_limits.clone(),
    };

    // Spawn Coordinator task
    let manager_arc = state.manager.clone();
    let app_handle = app.clone();
    let task_id_c = task_id.clone();
    let file_path_c = file_path.clone();

    tokio::spawn(async move {
        run_coordinator(
            manager_arc,
            app_handle,
            task_id_c,
            file_path_c,
            event_rx,
            Some(coordinator_ctx),
        )
        .await;
    });

    // Emit initial progress event
    let payload = ProgressPayload {
        id: task_id.clone(),
        status: TaskStatus::Downloading,
        downloaded_bytes: 0,
        total_bytes,
        progress: 0.0,
        speed: 0.0,
        chunks: chunks
            .iter()
            .map(|c| ChunkProgressSnapshot {
                index: c.index,
                downloaded: c.downloaded(),
                total: c.total_size(),
                speed: 0.0,
                completed: c.completed,
            })
            .collect(),
        eta_secs: 0.0,
    };

    let _ = app.emit("download_progress", &payload);

    Ok(task_id)
}

/// Context needed by the coordinator to dynamically spawn work-stealing workers.
#[derive(Clone)]
pub struct CoordinatorContext {
    pub client: reqwest::Client,
    pub throttler: BandwidthThrottler,
    pub event_tx: mpsc::Sender<WorkerEvent>,
    pub cancel_rx: watch::Receiver<bool>,
    pub custom_headers: Option<Vec<(String, String)>>,
    pub chunk_limits: downloader::ChunkLimitMap,
}

/// Download Coordinator Loop that processes worker events, updates metadata,
/// smooths speed, handles dynamic work stealing, and notifies the frontend.
async fn run_coordinator(
    manager: Arc<RwLock<DownloadManager>>,
    app: AppHandle,
    task_id: String,
    file_path: PathBuf,
    mut event_rx: mpsc::Receiver<WorkerEvent>,
    coordinator_ctx: Option<CoordinatorContext>,
) {
    let mut last_emit = tokio::time::Instant::now();
    let mut buffered_offsets: std::collections::HashMap<usize, u64> = std::collections::HashMap::new();
    let mut buffered_bytes: u64 = 0;

    while let Some(event) = event_rx.recv().await {
        let is_chunk_terminal = matches!(event, WorkerEvent::ChunkDone { .. } | WorkerEvent::ChunkError { .. });

        if let WorkerEvent::Progress { chunk_index, bytes_written, new_offset } = &event {
            buffered_offsets.insert(*chunk_index, *new_offset);
            buffered_bytes += bytes_written;
        }

        // Throttle write-lock acquisition to once every 100ms, or immediately on chunk terminal events
        if !is_chunk_terminal && last_emit.elapsed() < Duration::from_millis(100) {
            continue;
        }

        let pending_offsets = std::mem::take(&mut buffered_offsets);
        let pending_bytes = std::mem::replace(&mut buffered_bytes, 0);

        let (is_completed, payload) = {
            let mut mgr = manager.write();
            if let Some(handle) = mgr.tasks.get_mut(&task_id) {
                // Apply buffered progress
                for (idx, offset) in pending_offsets {
                    if idx < handle.metadata.chunks.len() {
                        handle.metadata.chunks[idx].current_byte = offset;
                    }
                }
                if pending_bytes > 0 {
                    handle.speed_meter.record(pending_bytes);
                }

                // Apply terminal events
                match event {
                    WorkerEvent::ChunkDone { chunk_index } => {
                        if chunk_index < handle.metadata.chunks.len() {
                            handle.metadata.chunks[chunk_index].completed = true;
                            handle.metadata.chunks[chunk_index].current_byte =
                                handle.metadata.chunks[chunk_index].end_byte + 1;
                        }

                        // Dynamic Work Stealing: An idle worker steals remaining work from longest active chunk
                        let all_chunks_done = handle.metadata.chunks.iter().all(|c| c.completed);
                        if !all_chunks_done && handle.metadata.supports_range && handle.metadata.chunks.len() < 32 {
                            if let Some(ref ctx) = coordinator_ctx {
                                if let Some((donor_idx, new_chunk)) = crate::engine::chunker::steal_work(&mut handle.metadata.chunks) {
                                    // Update donor chunk's atomic limit
                                    {
                                        let limits = ctx.chunk_limits.read();
                                        if let Some(donor_atomic) = limits.get(&donor_idx) {
                                            donor_atomic.store(handle.metadata.chunks[donor_idx].end_byte, std::sync::atomic::Ordering::SeqCst);
                                        }
                                    }
                                    // Register new chunk's atomic limit
                                    {
                                        let mut limits = ctx.chunk_limits.write();
                                        limits.insert(new_chunk.index, Arc::new(std::sync::atomic::AtomicU64::new(new_chunk.end_byte)));
                                    }

                                    let c_client = ctx.client.clone();
                                    let c_url = handle.metadata.url.clone();
                                    let c_chunk = new_chunk.clone();
                                    let c_path = file_path.clone();
                                    let c_throttler = ctx.throttler.clone();
                                    let c_tx = ctx.event_tx.clone();
                                    let c_cancel = ctx.cancel_rx.clone();
                                    let c_headers = ctx.custom_headers.clone();
                                    let c_limits = Some(ctx.chunk_limits.clone());

                                    handle.metadata.chunks.push(new_chunk);

                                    tokio::spawn(async move {
                                        downloader::download_chunk_with_retry(
                                            c_client,
                                            c_url,
                                            c_chunk,
                                            c_path,
                                            c_throttler,
                                            c_tx,
                                            c_cancel,
                                            c_headers,
                                            c_limits,
                                        ).await;
                                    });
                                }
                            }
                        }
                    }
                    WorkerEvent::ChunkError { chunk_index, error, will_retry } => {
                        eprintln!("[SuperIDM] Chunk {} error: {} (retry: {})", chunk_index, error, will_retry);
                    }
                    WorkerEvent::Progress { .. } => {}
                }

                let all_done = handle.metadata.chunks.iter().all(|c| c.completed);
                if all_done {
                    handle.metadata.status = TaskStatus::Completed;
                    let dld = handle.metadata.downloaded_bytes();
                    if handle.metadata.total_bytes == 0 || handle.metadata.total_bytes == 50 * 1024 * 1024 {
                        handle.metadata.total_bytes = dld;
                    }
                }

                let is_comp = handle.metadata.status == TaskStatus::Completed;
                let spd = if is_comp { 0.0 } else { handle.speed_meter.speed_bps() };
                let dld = if is_comp && handle.metadata.total_bytes > 0 { handle.metadata.total_bytes } else { handle.metadata.downloaded_bytes() };
                let tot = handle.metadata.total_bytes;
                let prog = if is_comp { 1.0 } else { handle.metadata.progress() };
                let eta = if !is_comp && spd > 0.0 && tot > dld {
                    (tot - dld) as f64 / spd
                } else {
                    0.0
                };

                let pl = ProgressPayload {
                    id: task_id.clone(),
                    status: handle.metadata.status.clone(),
                    downloaded_bytes: dld,
                    total_bytes: tot,
                    progress: prog,
                    speed: spd,
                    chunks: handle
                        .metadata
                        .chunks
                        .iter()
                        .map(|c| ChunkProgressSnapshot {
                            index: c.index,
                            downloaded: c.downloaded(),
                            total: c.total_size(),
                            speed: 0.0,
                            completed: c.completed,
                        })
                        .collect(),
                    eta_secs: eta,
                };

                (all_done, Some(pl))
            } else {
                (false, None)
            }
        };

        if let Some(pl) = payload {
            if is_completed {
                // Truncate and flush physical file to exact downloaded size to guarantee video playability
                if let Ok(file) = storage::open_shared_file(&file_path) {
                    let actual_len = pl.downloaded_bytes;
                    if actual_len > 0 {
                        let _ = file.set_len(actual_len).await;
                        let _ = file.sync_all().await;
                    }
                }
                let _ = app.emit("download_progress", &pl);
                let _ = storage::delete_part_json(&file_path).await;
                let (task_fname, all_tasks): (String, Vec<DownloadMetadata>) = {
                    let mgr = manager.read();
                    let fname = mgr.tasks.get(&task_id).map(|h| h.metadata.filename.clone()).unwrap_or_else(|| "الملف".to_string());
                    let tasks: Vec<DownloadMetadata> = mgr.tasks.values().map(|h| h.metadata.clone()).collect();
                    (fname, tasks)
                };
                let _ = storage::save_tasks_db(&all_tasks).await;

                // Native Windows Toast Notification on download complete
                use tauri_plugin_notification::NotificationExt;
                let _ = app.notification()
                    .builder()
                    .title("SuperIDM — اكتمل التحميل")
                    .body(format!("اكتمل تحميل الملف بنجاح: {}", task_fname))
                    .show();

                break;
            } else {
                let _ = app.emit("download_progress", &pl);
                last_emit = tokio::time::Instant::now();
            }
        }
    }

    // If the coordinator loop exits without is_completed (e.g. workers exhausted retries, server 403, or connection drop):
    let failure_payload = {
        let mut mgr = manager.write();
        if let Some(handle) = mgr.tasks.get_mut(&task_id) {
            if handle.metadata.status == TaskStatus::Downloading {
                handle.metadata.status = TaskStatus::Failed;
                handle.speed_meter = SpeedMeter::new();
                Some(ProgressPayload {
                    id: task_id.clone(),
                    status: TaskStatus::Failed,
                    downloaded_bytes: handle.metadata.downloaded_bytes(),
                    total_bytes: handle.metadata.total_bytes,
                    progress: handle.metadata.progress(),
                    speed: 0.0,
                    chunks: handle
                        .metadata
                        .chunks
                        .iter()
                        .map(|c| ChunkProgressSnapshot {
                            index: c.index,
                            downloaded: c.downloaded(),
                            total: c.total_size(),
                            speed: 0.0,
                            completed: c.completed,
                        })
                        .collect(),
                    eta_secs: 0.0,
                })
            } else {
                None
            }
        } else {
            None
        }
    };

    if let Some(pl) = failure_payload {
        let _ = app.emit("download_progress", &pl);
        let all_tasks: Vec<_> = manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
        let _ = storage::save_tasks_db(&all_tasks).await;
    }
}

/// Pause a download task.
#[tauri::command]
pub async fn pause_download(
    state: State<'_, AppState>,
    app: AppHandle,
    task_id: Option<String>,
    #[allow(non_snake_case)]
    taskId: Option<String>,
) -> Result<(), String> {
    let id = task_id.or(taskId).ok_or_else(|| "Missing task_id".to_string())?;
    let (file_path_opt, metadata_opt, payload_opt) = {
        let mut mgr = state.manager.write();
        if let Some(handle) = mgr.tasks.get_mut(&id) {
            let _ = handle.cancel_token.send(true);
            handle.metadata.status = TaskStatus::Paused;
            handle.speed_meter = SpeedMeter::new();
            let file_path = PathBuf::from(&handle.metadata.destination);
            let meta = handle.metadata.clone();

            let pl = ProgressPayload {
                id: id.clone(),
                status: TaskStatus::Paused,
                downloaded_bytes: handle.metadata.downloaded_bytes(),
                total_bytes: handle.metadata.total_bytes,
                progress: handle.metadata.progress(),
                speed: 0.0,
                chunks: handle.metadata.chunks.iter().map(|c| ChunkProgressSnapshot {
                    index: c.index,
                    downloaded: c.downloaded(),
                    total: c.total_size(),
                    speed: 0.0,
                    completed: c.completed,
                }).collect(),
                eta_secs: 0.0,
            };
            (Some(file_path), Some(meta), Some(pl))
        } else {
            (None, None, None)
        }
    };

    if let (Some(fp), Some(meta), Some(pl)) = (file_path_opt, metadata_opt, payload_opt) {
        let is_media = crate::engine::extractor::MediaExtractor::is_media_url(&meta.url)
            || meta.format_id.is_some()
            || meta.chunks.len() <= 1 && meta.chunks.first().map(|c| c.end_byte == 0).unwrap_or(true);
        if !is_media {
            let _ = storage::save_part_json_atomic(&fp, &meta).await;
        }
        let _ = app.emit("download_progress", &pl);
        let all_tasks: Vec<_> = state.manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
        let _ = storage::save_tasks_db(&all_tasks).await;
    }

    Ok(())
}

/// Resume a download task.
#[tauri::command]
pub async fn resume_download(
    state: State<'_, AppState>,
    app: AppHandle,
    task_id: Option<String>,
    #[allow(non_snake_case)]
    taskId: Option<String>,
) -> Result<(), String> {
    let id = task_id.or(taskId).ok_or_else(|| "Missing task_id".to_string())?;
    let (metadata, client, throttler) = {
        let mut mgr = state.manager.write();
        if let Some(handle) = mgr.tasks.get_mut(&id) {
            handle.metadata.status = TaskStatus::Downloading;
            let (cancel_tx, _) = watch::channel(false);
            handle.cancel_token = cancel_tx;
            (
                handle.metadata.clone(),
                mgr.http_client.clone(),
                mgr.throttler.clone(),
            )
        } else {
            return Err("Task not found".into());
        }
    };

    let (cancel_tx, cancel_rx) = watch::channel(false);
    {
        let mut mgr = state.manager.write();
        if let Some(handle) = mgr.tasks.get_mut(&id) {
            handle.cancel_token = cancel_tx;
        }
    }

    // If media stream extractor task: resume stream extractor
    if crate::engine::extractor::MediaExtractor::is_media_url(&metadata.url)
        || metadata.format_id.is_some()
        || metadata.chunks.is_empty()
    {
        let dest_path = PathBuf::from(&metadata.destination);
        let dest_dir = if dest_path.is_dir() {
            dest_path
        } else {
            dest_path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| state.manager.read().default_download_dir.clone())
        };
        let (prog_tx, mut prog_rx) = mpsc::channel(128);
        let url_c = metadata.url.clone();
        let format_id_c = metadata.format_id.clone();
        let dest_dir_ext = dest_dir.clone();
        let dest_dir_watch = dest_dir.clone();
        let cancel_c = cancel_rx.clone();

        let client_c = state.manager.read().http_client.clone();
        let mgr_err_arc = state.manager.clone();
        let app_err = app.clone();
        let task_id_err = id.clone();
        tokio::spawn(async move {
            if let Err(e) = crate::engine::extractor::MediaExtractor::download_media_stream(
                &client_c,
                &url_c,
                &dest_dir_ext,
                format_id_c.as_deref(),
                &[],
                prog_tx,
                cancel_c,
            ).await {
                let was_user_paused = {
                    let mgr = mgr_err_arc.read();
                    mgr.tasks
                        .get(&task_id_err)
                        .map(|h| h.metadata.status == TaskStatus::Paused)
                        .unwrap_or(true)
                };
                if !was_user_paused && !e.to_lowercase().contains("cancelled") {
                    eprintln!("[MediaExtractor Resume Error] {}", e);
                    let _ = app_err.emit("download_error", serde_json::json!({
                        "id": task_id_err,
                        "message": e,
                    }));
                }
            }
        });

        let app_handle = app.clone();
        let task_id_c = id.clone();
        let mgr_arc = state.manager.clone();

        tokio::spawn(async move {
            let mut completed = false;
            let mut prev_downloaded = 0u64;
            while let Some((progress, speed, downloaded, total, eta_secs, is_comp, final_name_opt)) = prog_rx.recv().await {
                let raw_total = if total > 0 { total } else if downloaded > 0 { downloaded } else { 0 };
                let eff_downloaded = downloaded;
                let eff_total = raw_total.max(eff_downloaded);
                {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        if eff_downloaded > prev_downloaded {
                            h.speed_meter.record(eff_downloaded - prev_downloaded);
                            prev_downloaded = eff_downloaded;
                        }
                        h.metadata.total_bytes = eff_total;
                        if let Some(ref fname) = final_name_opt {
                            h.metadata.filename = fname.clone();
                            let new_dest = dest_dir_watch.join(fname);
                            h.metadata.destination = new_dest.to_string_lossy().to_string();
                        }
                        let prog_bp = (progress.clamp(0.0, 1.0) * 10000.0) as usize;
                        if h.metadata.chunks.is_empty() {
                            h.metadata.chunks.push(ChunkInfo {
                                index: 0,
                                start_byte: 0,
                                end_byte: eff_total,
                                current_byte: eff_downloaded,
                                completed: is_comp,
                                retry_count: prog_bp,
                            });
                        } else {
                            h.metadata.chunks[0].current_byte = eff_downloaded;
                            h.metadata.chunks[0].end_byte = eff_total;
                            h.metadata.chunks[0].completed = is_comp;
                            h.metadata.chunks[0].retry_count = prog_bp;
                        }
                        if is_comp {
                            h.metadata.status = TaskStatus::Completed;
                            h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
                            h.speed_meter.reset();
                            completed = true;
                        }
                    }
                }

                let payload = ProgressPayload {
                    id: task_id_c.clone(),
                    status: if is_comp { TaskStatus::Completed } else { TaskStatus::Downloading },
                    downloaded_bytes: eff_downloaded,
                    total_bytes: eff_total,
                    progress,
                    speed: if is_comp { 0.0 } else { speed },
                    chunks: vec![],
                    eta_secs: if is_comp { 0.0 } else { eta_secs },
                };

                let _ = app_handle.emit("download_progress", &payload);

                if is_comp {
                    let all_tasks: Vec<_> = mgr_arc.read().tasks.values().map(|h| h.metadata.clone()).collect();
                    let _ = storage::save_tasks_db(&all_tasks).await;
                    break;
                }
            }

            if !completed {
                let maybe_fail = {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        if h.metadata.status != TaskStatus::Completed && h.metadata.status != TaskStatus::Paused {
                            h.metadata.status = TaskStatus::Failed;
                            h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
                            Some(ProgressPayload {
                                id: task_id_c.clone(),
                                status: TaskStatus::Failed,
                                downloaded_bytes: 0,
                                total_bytes: 0,
                                progress: 0.0,
                                speed: 0.0,
                                chunks: vec![],
                                eta_secs: 0.0,
                            })
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                };
                if let Some(payload) = maybe_fail {
                    let _ = app_handle.emit("download_progress", &payload);
                    let all_tasks: Vec<_> = mgr_arc.read().tasks.values().map(|h| h.metadata.clone()).collect();
                    let _ = storage::save_tasks_db(&all_tasks).await;
                }
            }
        });

        return Ok(());
    }

    // Direct multi-chunk resume with sidecar recovery, ETag consistency check, and work stealing
    let file_path = PathBuf::from(&metadata.destination);
    let mut resume_meta = metadata.clone();

    // 1. Recover fresher chunk offsets from `.part.json` sidecar if present
    if let Ok(Some(sidecar)) = storage::load_part_json(&file_path).await {
        if sidecar.url == resume_meta.url
            && !sidecar.chunks.is_empty()
            && sidecar.downloaded_bytes() > resume_meta.downloaded_bytes()
        {
            resume_meta.chunks = sidecar.chunks;
            resume_meta.total_bytes = sidecar.total_bytes.max(resume_meta.total_bytes);
            if resume_meta.etag.is_none() {
                resume_meta.etag = sidecar.etag;
            }
            if resume_meta.last_modified.is_none() {
                resume_meta.last_modified = sidecar.last_modified;
            }
            let mut mgr = state.manager.write();
            if let Some(handle) = mgr.tasks.get_mut(&id) {
                handle.metadata.chunks = resume_meta.chunks.clone();
                handle.metadata.total_bytes = resume_meta.total_bytes;
            }
        }
    }

    // 2. Verify remote file hasn't changed on the server (ETag / Last-Modified / Content-Length)
    if let Ok(probe) = downloader::probe_url(&client, &resume_meta.url).await {
        if !downloader::validate_consistency(&resume_meta, &probe) {
            crate::engine::extractor::record_engine_log(format!(
                "[resume] Remote file changed on server for {}; resetting chunks for clean download",
                resume_meta.filename
            ));
            let new_total = probe.content_length;
            let new_chunks = if new_total > 0 {
                let effective_conns = if probe.supports_range { resume_meta.num_connections } else { 1 };
                chunker::calculate_chunks(new_total, effective_conns)
            } else {
                vec![ChunkInfo {
                    index: 0,
                    start_byte: 0,
                    end_byte: 0,
                    current_byte: 0,
                    completed: false,
                    retry_count: 0,
                }]
            };
            if new_total > 0 {
                let _ = storage::preallocate_file(&file_path, new_total).await;
            }
            resume_meta.total_bytes = new_total;
            resume_meta.etag = probe.etag;
            resume_meta.last_modified = probe.last_modified;
            resume_meta.supports_range = probe.supports_range;
            resume_meta.chunks = new_chunks;
            let mut mgr = state.manager.write();
            if let Some(handle) = mgr.tasks.get_mut(&id) {
                handle.metadata.total_bytes = resume_meta.total_bytes;
                handle.metadata.etag = resume_meta.etag.clone();
                handle.metadata.last_modified = resume_meta.last_modified.clone();
                handle.metadata.supports_range = resume_meta.supports_range;
                handle.metadata.chunks = resume_meta.chunks.clone();
            }
        }
    }

    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>(4096);
    let chunk_limits: downloader::ChunkLimitMap = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    for chunk in &resume_meta.chunks {
        if chunk.completed {
            continue;
        }
        let client_c = client.clone();
        let url_c = resume_meta.url.clone();
        let chunk_c = chunk.clone();
        let file_path_c = file_path.clone();
        let throttler_c = throttler.clone();
        let cancel_rx_c = cancel_rx.clone();
        let tx = event_tx.clone();
        let limits_c = Some(chunk_limits.clone());

        tokio::spawn(async move {
            downloader::download_chunk_with_retry(
                client_c,
                url_c,
                chunk_c,
                file_path_c,
                throttler_c,
                tx,
                cancel_rx_c,
                None,
                limits_c,
            )
            .await;
        });
    }

    let coordinator_ctx = CoordinatorContext {
        client: client.clone(),
        throttler: throttler.clone(),
        event_tx: event_tx.clone(),
        cancel_rx: cancel_rx.clone(),
        custom_headers: None,
        chunk_limits: chunk_limits.clone(),
    };

    let manager_arc = state.manager.clone();
    let app_handle = app.clone();
    let task_id_c = id.clone();
    let file_path_c = file_path.clone();

    tokio::spawn(async move {
        run_coordinator(
            manager_arc,
            app_handle,
            task_id_c,
            file_path_c,
            event_rx,
            Some(coordinator_ctx),
        )
        .await;
    });

    Ok(())
}

fn normalize_alnum_stem(stem: &str) -> String {
    stem.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Resolve all candidate file paths for a task (never returning a bare directory), including
/// normalized stem matching in case Windows / yt-dlp sanitized punctuation (`|` -> `｜`, `?` -> `？`).
pub fn resolve_candidate_file_paths(
    raw_dest: Option<&str>,
    raw_filename: Option<&str>,
    default_dir: &std::path::Path,
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let mut dirs_to_search: Vec<PathBuf> = Vec::new();

    if let Some(d_str) = raw_dest.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let dp = PathBuf::from(d_str);
        if dp.is_dir() {
            dirs_to_search.push(dp.clone());
            if let Some(fname) = raw_filename.map(|s| s.trim()).filter(|s| !s.is_empty()) {
                candidates.push(dp.join(fname));
            }
        } else {
            candidates.push(dp.clone());
            if let Some(parent) = dp.parent() {
                if parent.is_dir() {
                    dirs_to_search.push(parent.to_path_buf());
                    if let Some(fname) = raw_filename.map(|s| s.trim()).filter(|s| !s.is_empty()) {
                        let p_fname = parent.join(fname);
                        if !candidates.contains(&p_fname) {
                            candidates.push(p_fname);
                        }
                    }
                }
            }
        }
    }

    if let Some(fname) = raw_filename.map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let in_default = default_dir.join(fname);
        if !candidates.contains(&in_default) {
            candidates.push(in_default);
        }
        if default_dir.is_dir() && !dirs_to_search.iter().any(|d| d == default_dir) {
            dirs_to_search.push(default_dir.to_path_buf());
        }
    }

    // If none of the exact candidates exist, check sanitized-punctuation equivalent in search dirs
    if !candidates.iter().any(|c| c.is_file()) {
        let reference_name = raw_filename
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| {
                candidates
                    .first()
                    .and_then(|p| p.file_name())
                    .map(|n| n.to_string_lossy().to_string())
            });
        if let Some(ref_name) = reference_name {
            let ref_path = std::path::Path::new(&ref_name);
            let target_ext = ref_path
                .extension()
                .and_then(|e| e.to_str())
                .map(|s| s.to_lowercase());
            let target_norm = ref_path
                .file_stem()
                .map(|s| normalize_alnum_stem(&s.to_string_lossy()))
                .unwrap_or_default();

            if target_norm.chars().count() >= 6 {
                for dir in dirs_to_search {
                    if let Ok(entries) = std::fs::read_dir(&dir) {
                        for entry in entries.flatten() {
                            let ep = entry.path();
                            if !ep.is_file() {
                                continue;
                            }
                            let ep_ext = ep.extension().and_then(|e| e.to_str()).map(|s| s.to_lowercase());
                            if target_ext.is_some() && ep_ext != target_ext {
                                continue;
                            }
                            if let Some(estem) = ep.file_stem() {
                                if normalize_alnum_stem(&estem.to_string_lossy()) == target_norm {
                                    if !candidates.contains(&ep) {
                                        candidates.push(ep);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    candidates
}

async fn remove_file_with_retry(path: &std::path::Path) -> bool {
    if !path.exists() || path.is_dir() {
        return false;
    }
    for attempt in 0..6u64 {
        match tokio::fs::remove_file(path).await {
            Ok(()) => return true,
            Err(_) if attempt < 5 => {
                tokio::time::sleep(Duration::from_millis(120)).await;
            }
            Err(_) => return false,
        }
    }
    false
}

/// Remove the target file and/or associated temp fragments (`.part`, `.ytdl`, `.fNNN.*`).
pub async fn cleanup_task_files(
    candidates: &[PathBuf],
    delete_completed_file: bool,
) -> Vec<String> {
    let mut deleted = Vec::new();

    for path in candidates {
        if path.is_dir() {
            continue;
        }
        let _ = storage::delete_part_json(path).await;

        let part_path = PathBuf::from(format!("{}.part", path.to_string_lossy()));
        if remove_file_with_retry(&part_path).await {
            deleted.push(part_path.to_string_lossy().to_string());
        }
        let ytdl_path = PathBuf::from(format!("{}.ytdl", path.to_string_lossy()));
        if remove_file_with_retry(&ytdl_path).await {
            deleted.push(ytdl_path.to_string_lossy().to_string());
        }

        if let (Some(parent), Some(stem)) = (path.parent(), path.file_stem().and_then(|s| s.to_str())) {
            if parent.is_dir() && !stem.is_empty() {
                if let Ok(mut entries) = tokio::fs::read_dir(parent).await {
                    let prefix_dot = format!("{}.", stem);
                    while let Ok(Some(entry)) = entries.next_entry().await {
                        let ep = entry.path();
                        if !ep.is_file() {
                            continue;
                        }
                        if let Some(name) = ep.file_name().and_then(|n| n.to_str()) {
                            let low = name.to_lowercase();
                            let is_yt_temp = name.starts_with(&prefix_dot)
                                && (low.contains(".part")
                                    || low.ends_with(".ytdl")
                                    || low.contains(".temp.")
                                    || name[prefix_dot.len()..].starts_with('f')
                                        && name[prefix_dot.len() + 1..]
                                            .split('.')
                                            .next()
                                            .map(|d| !d.is_empty() && d.chars().all(|c| c.is_ascii_digit() || c == '-'))
                                            .unwrap_or(false));
                            if is_yt_temp && remove_file_with_retry(&ep).await {
                                deleted.push(ep.to_string_lossy().to_string());
                            }
                        }
                    }
                }
            }
        }

        if delete_completed_file && path.is_file() && remove_file_with_retry(path).await {
            deleted.push(path.to_string_lossy().to_string());
        }
    }

    deleted
}

/// Cancel / delete a download task and optionally delete its associated file on disk.
#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    task_id: Option<String>,
    #[allow(non_snake_case)]
    taskId: Option<String>,
    destination: Option<String>,
    filename: Option<String>,
    delete_file: Option<bool>,
    #[allow(non_snake_case)]
    deleteFile: Option<bool>,
) -> Result<serde_json::Value, String> {
    let id = task_id.or(taskId).ok_or_else(|| "Missing task_id".to_string())?;
    let should_delete_file = delete_file.or(deleteFile).unwrap_or(true);

    let (removed_meta, default_dir) = {
        let mut mgr = state.manager.write();
        let def = mgr.default_download_dir.clone();
        if let Some(handle) = mgr.tasks.remove(&id) {
            let _ = handle.cancel_token.send(true);
            (Some(handle.metadata), def)
        } else {
            (None, def)
        }
    };

    let was_active = removed_meta
        .as_ref()
        .map(|m| m.status == TaskStatus::Downloading)
        .unwrap_or(false);
    if was_active {
        // Give child processes (`taskkill /T`) a moment to terminate and close handles
        tokio::time::sleep(Duration::from_millis(180)).await;
    }

    let meta_dest = removed_meta.as_ref().map(|m| m.destination.as_str());
    let meta_fname = removed_meta.as_ref().map(|m| m.filename.as_str());

    let mut candidates = resolve_candidate_file_paths(
        meta_dest.or(destination.as_deref()),
        meta_fname.or(filename.as_deref()),
        &default_dir,
    );
    if destination.is_some() || filename.is_some() {
        for c in resolve_candidate_file_paths(destination.as_deref(), filename.as_deref(), &default_dir) {
            if !candidates.contains(&c) {
                candidates.push(c);
            }
        }
    }

    let deleted_paths = cleanup_task_files(&candidates, should_delete_file).await;

    let all_tasks: Vec<_> = state.manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
    let _ = storage::save_tasks_db(&all_tasks).await;

    Ok(serde_json::json!({
        "removed": true,
        "deleted_file": !deleted_paths.is_empty(),
        "deleted_paths": deleted_paths,
    }))
}

/// Get all tasks as summaries for the frontend dashboard.
#[tauri::command]
pub fn get_all_tasks(state: State<'_, AppState>) -> Vec<TaskSummary> {
    let mgr = state.manager.read();
    mgr.tasks
        .values()
        .map(|h| {
            let meta = &h.metadata;
            let spd = h.speed_meter.speed_bps();
            let dld = meta.downloaded_bytes();
            let tot = meta.total_bytes;
            TaskSummary {
                id: meta.id.clone(),
                filename: meta.filename.clone(),
                url: meta.url.clone(),
                destination: meta.destination.clone(),
                total_bytes: tot,
                downloaded_bytes: dld,
                progress: if meta.status == TaskStatus::Completed {
                    1.0
                } else if meta.chunks.len() == 1 && meta.chunks[0].retry_count > 10 {
                    ((meta.chunks[0].retry_count as f64) / 10000.0).min(0.99)
                } else if tot > 0 {
                    meta.progress().min(0.99)
                } else {
                    0.0
                },
                speed: spd,
                status: meta.status.clone(),
                category: meta.category.clone(),
                priority: meta.priority,
                num_connections: meta.num_connections,
                supports_range: meta.supports_range,
                created_at: meta.created_at,
                eta_secs: if spd > 0.0 && tot > dld {
                    (tot - dld) as f64 / spd
                } else {
                    0.0
                },
            }
        })
        .collect()
}

/// Set global bandwidth throttle limit.
#[tauri::command]
pub async fn set_global_throttle(
    state: State<'_, AppState>,
    limit_bytes_per_sec: Option<u64>,
    #[allow(non_snake_case)]
    limitBytesPerSec: Option<u64>,
) -> Result<(), String> {
    let limit = limit_bytes_per_sec.or(limitBytesPerSec).unwrap_or(0);
    let throttler = state.manager.read().throttler.clone();
    throttler.set_limit(limit).await;
    Ok(())
}

/// Open a folder in Windows Explorer (highlighting the target file when present).
#[tauri::command]
pub async fn open_download_folder(
    state: State<'_, AppState>,
    path: Option<String>,
    #[allow(non_snake_case)]
    folderPath: Option<String>,
    filename: Option<String>,
) -> Result<(), String> {
    let default_downloads = state.manager.read().default_download_dir.clone();
    let raw_target = path.or(folderPath).unwrap_or_default().trim().to_string();

    let candidates = resolve_candidate_file_paths(
        if raw_target.is_empty() { None } else { Some(raw_target.as_str()) },
        filename.as_deref(),
        &default_downloads,
    );
    let existing_file = candidates.into_iter().find(|c| c.is_file());

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        if let Some(file_pb) = existing_file {
            let arg = format!("/select,\"{}\"", file_pb.to_string_lossy());
            let mut cmd = std::process::Command::new("explorer.exe");
            cmd.raw_arg(arg);
            let _ = cmd.spawn();
        } else {
            let raw_pb = PathBuf::from(&raw_target);
            let dir_to_open = if raw_pb.is_dir() {
                raw_pb
            } else if let Some(parent) = raw_pb.parent().filter(|p| p.is_dir() && p.parent().is_some() && raw_pb.is_absolute()) {
                parent.to_path_buf()
            } else {
                default_downloads
            };
            let _ = std::process::Command::new("explorer.exe")
                .arg(&dir_to_open)
                .spawn();
        }
    }

    Ok(())
}

/// Open a downloaded file with its default associated Windows application.
#[tauri::command]
pub async fn open_file(
    state: State<'_, AppState>,
    path: Option<String>,
    #[allow(non_snake_case)]
    filePath: Option<String>,
    filename: Option<String>,
) -> Result<(), String> {
    let default_downloads = state.manager.read().default_download_dir.clone();
    let raw_target = path.or(filePath).unwrap_or_default().trim().to_string();

    let candidates = resolve_candidate_file_paths(
        if raw_target.is_empty() { None } else { Some(raw_target.as_str()) },
        filename.as_deref(),
        &default_downloads,
    );
    let target_file = candidates
        .into_iter()
        .find(|c| c.is_file())
        .ok_or_else(|| {
            let display = filename.unwrap_or(raw_target);
            format!("File not found on disk: {}", display)
        })?;

    #[cfg(target_os = "windows")]
    {
        let path_str = target_file.to_string_lossy().to_string();
        std::process::Command::new("cmd")
            .args(["/c", "start", "", &path_str])
            .spawn()
            .map_err(|e| format!("Failed to open file: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = open::that(&target_file);
    }

    Ok(())
}

/// Extract media metadata, format options, and direct streams from any URL.
#[tauri::command]
pub async fn extract_media_info(
    state: State<'_, AppState>,
    url: String,
) -> Result<MediaExtractionInfo, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    crate::engine::extractor::MediaExtractor::extract_info(&client, &url).await
}

/// Extract all items and metadata from a video/media playlist (YouTube playlist, channel, etc.).
#[tauri::command]
pub async fn extract_playlist(
    state: State<'_, AppState>,
    url: String,
) -> Result<PlaylistExtractionInfo, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    crate::engine::extractor::MediaExtractor::extract_playlist_info(&client, &url).await
}

/// Start an adaptive HLS/DASH streaming download with auto-muxing.
#[tauri::command]
pub async fn start_stream_download(
    state: State<'_, AppState>,
    app: AppHandle,
    request: serde_json::Value,
) -> Result<String, String> {
    let req_obj = if let Some(inner) = request.get("request") { inner } else { &request };

    let url = req_obj.get("url").and_then(|v| v.as_str()).ok_or_else(|| "Missing 'url'".to_string())?.to_string();
    let filename = req_obj.get("filename").and_then(|v| v.as_str()).unwrap_or("stream_video.mp4").to_string();
    let destination = req_obj.get("destination").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let category = req_obj.get("category").and_then(|v| v.as_str()).unwrap_or("Videos").to_string();

    let task_id = Uuid::new_v4().to_string();
    let default_dir = state.manager.read().default_download_dir.clone();
    let dest_dir = if destination.is_empty() || destination.contains("Users\\User") {
        default_dir
    } else {
        PathBuf::from(&destination)
    };

    let _ = tokio::fs::create_dir_all(&dest_dir).await;
    let file_path = dest_dir.join(&filename);

    let (cancel_tx, cancel_rx) = watch::channel(false);
    let (prog_tx, mut prog_rx) = mpsc::channel::<(usize, usize, u64)>(128);

    let client = state.manager.read().http_client.clone();
    let now = chrono::Utc::now().timestamp_millis();

    let metadata = DownloadMetadata {
        id: task_id.clone(),
        url: url.clone(),
        destination: file_path.to_string_lossy().to_string(),
        filename: filename.clone(),
        total_bytes: 0,
        status: TaskStatus::Downloading,
        etag: None,
        last_modified: None,
        supports_range: true,
        num_connections: 8,
        priority: 1,
        category,
        format_id: None,
        chunks: Vec::new(),
        created_at: now,
        updated_at: now,
    };

    {
        let mut mgr = state.manager.write();
        mgr.tasks.insert(
            task_id.clone(),
            TaskHandle {
                metadata,
                speed_meter: SpeedMeter::new(),
                cancel_token: cancel_tx,
            },
        );
    }

    let client_c = client.clone();
    let url_c = url.clone();
    let out_c = file_path.clone();
    let cancel_c = cancel_rx.clone();

    // Spawn stream worker
    tokio::spawn(async move {
        let _ = crate::engine::stream::StreamEngine::download_hls_stream(
            client_c,
            url_c,
            out_c,
            8,
            prog_tx,
            cancel_c,
        ).await;
    });

    // Progress watcher
    let app_handle = app.clone();
    let task_id_c = task_id.clone();
    let mgr_arc = state.manager.clone();

    tokio::spawn(async move {
        let mut speed_meter = SpeedMeter::new();
        while let Some((done, total, total_bytes)) = prog_rx.recv().await {
            let progress = if total > 0 { done as f64 / total as f64 } else { 0.0 };
            speed_meter.record(total_bytes);

            let is_comp = done >= total;
            let spd = speed_meter.speed_bps();

            {
                let mut mgr = mgr_arc.write();
                if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                    h.metadata.total_bytes = (total_bytes as f64 / progress.max(0.01)) as u64;
                    if h.metadata.chunks.is_empty() {
                        h.metadata.chunks.push(ChunkInfo {
                            index: 0,
                            start_byte: 0,
                            end_byte: h.metadata.total_bytes,
                            current_byte: total_bytes,
                            completed: is_comp,
                            retry_count: 0,
                        });
                    } else {
                        h.metadata.chunks[0].current_byte = total_bytes;
                        h.metadata.chunks[0].completed = is_comp;
                    }
                    if is_comp {
                        h.metadata.status = TaskStatus::Completed;
                    }
                }
            }

            let payload = ProgressPayload {
                id: task_id_c.clone(),
                status: if is_comp { TaskStatus::Completed } else { TaskStatus::Downloading },
                downloaded_bytes: total_bytes,
                total_bytes: (total_bytes as f64 / progress.max(0.01)) as u64,
                progress,
                speed: if is_comp { 0.0 } else { spd },
                chunks: vec![],
                eta_secs: if spd > 0.0 && progress > 0.001 && progress < 1.0 { ((1.0 - progress) * total_bytes as f64 / progress) / spd } else { 0.0 },
            };

            let _ = app_handle.emit("download_progress", &payload);

            if is_comp {
                break;
            }
        }
    });

    Ok(task_id)
}

/// Refresh an expired temporary signed URL using the original source link or a new direct URL.
#[tauri::command]
pub async fn refresh_task_link(
    state: State<'_, AppState>,
    task_id: Option<String>,
    #[allow(non_snake_case)]
    taskId: Option<String>,
    new_url: Option<String>,
    #[allow(non_snake_case)]
    newUrl: Option<String>,
    original_url: Option<String>,
    #[allow(non_snake_case)]
    originalUrl: Option<String>,
) -> Result<String, String> {
    let id = task_id.or(taskId).ok_or_else(|| "Missing task_id".to_string())?;
    let provided_new = new_url.or(newUrl);
    let provided_orig = original_url.or(originalUrl);

    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    let fresh_url = if let Some(trimmed) = provided_new.as_deref().map(|s| s.trim()).filter(|s| !s.is_empty()) {
        trimmed.to_string()
    } else {
        let (source_url, mut placeholder) = {
            let mgr = state.manager.read();
            if let Some(h) = mgr.tasks.get(&id) {
                let s_url = provided_orig.unwrap_or_else(|| h.metadata.url.clone());
                (s_url, h.metadata.clone())
            } else {
                let s_url = provided_orig.ok_or_else(|| "Task not found".to_string())?;
                let now = chrono::Utc::now().timestamp_millis();
                let placeholder = DownloadMetadata {
                    id: id.clone(),
                    url: s_url.clone(),
                    destination: String::new(),
                    filename: String::new(),
                    total_bytes: 0,
                    status: TaskStatus::Paused,
                    etag: None,
                    last_modified: None,
                    supports_range: true,
                    num_connections: 1,
                    priority: 0,
                    category: "Videos".to_string(),
                    format_id: None,
                    chunks: vec![],
                    created_at: now,
                    updated_at: now,
                };
                (s_url, placeholder)
            }
        };
        crate::engine::recovery::RecoveryEngine::refresh_expired_link(
            &client,
            &source_url,
            &mut placeholder,
        ).await?
    };

    let updated_in_mgr = {
        let mut mgr = state.manager.write();
        if let Some(h) = mgr.tasks.get_mut(&id) {
            h.metadata.url = fresh_url.clone();
            h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
            if h.metadata.status == TaskStatus::Failed {
                h.metadata.status = TaskStatus::Paused;
            }
            let dest = PathBuf::from(&h.metadata.destination);
            let meta = h.metadata.clone();
            let all = mgr.tasks.values().map(|t| t.metadata.clone()).collect::<Vec<_>>();
            Some((dest, meta, all))
        } else {
            None
        }
    };

    if let Some((dest_path, meta_clone, all_tasks)) = updated_in_mgr {
        if !meta_clone.chunks.is_empty() && !crate::engine::extractor::MediaExtractor::is_media_url(&meta_clone.url) {
            let _ = storage::save_part_json_atomic(&dest_path, &meta_clone).await;
        }
        let _ = storage::save_tasks_db(&all_tasks).await;
    }

    Ok(fresh_url)
}

/// Send a native Windows Toast notification on download complete.
#[tauri::command]
pub async fn notify_download_complete(
    app: AppHandle,
    title: String,
    body: String,
) -> Result<(), String> {
    use tauri_plugin_notification::NotificationExt;
    app.notification()
        .builder()
        .title(title)
        .body(body)
        .show()
        .map_err(|e| e.to_string())
}

/// Open the extension folder in Windows Explorer so the user can load it manually.
#[tauri::command]
pub async fn open_extension_folder() -> Result<(), String> {
    let target_dir = crate::engine::browser_integration::get_embedded_extension_dir();

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(&target_dir)
            .spawn()
            .map_err(|e| format!("Failed to open extension folder: {}", e))?;
    }

    Ok(())
}

/// Poll for pending RPC download requests from the browser extension.
/// Automatically unminimizes and brings the window to the front when a new request arrives!
#[tauri::command]
pub fn poll_rpc_downloads(
    app: AppHandle,
    state: State<'_, AppState>,
) -> Vec<serde_json::Value> {
    let mut mgr = state.manager.write();
    let pending: Vec<_> = mgr.pending_rpc_downloads.drain(..).collect();
    if !pending.is_empty() {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_always_on_top(true);
            let _ = window.set_focus();
            let _ = window.set_always_on_top(false);
        }
    }
    pending
        .into_iter()
        .map(|req| {
            serde_json::json!({
                "task_id": req.task_id,
                "url": req.url,
                "filename": req.filename,
                "format": req.format,
            })
        })
        .collect()
}

/// Explicitly bring the main window to the front and focus it.
#[tauri::command]
pub fn focus_window(app: AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
        let _ = window.set_always_on_top(false);
    }
    Ok(())
}

/// Get the active RPC security token for browser extension authentication.
#[tauri::command]
pub fn get_rpc_token() -> String {
    crate::engine::rpc::get_or_create_rpc_token()
}

/// Automatically install and register the SuperIDM extension into Chrome/Edge/Brave.
#[tauri::command]
pub fn install_browser_integration(browser: Option<String>) -> Result<String, String> {
    let token = crate::engine::rpc::get_or_create_rpc_token();
    let ext_dir = crate::engine::browser_integration::extract_and_sync_extension(&token)?;
    let target_browser = browser.unwrap_or_else(|| "all".to_string());
    crate::engine::browser_integration::register_browser_registry(&target_browser)?;
    Ok(format!("Extension ready at: {}", ext_dir.display()))
}

/// Get the path to the extracted extension directory.
#[tauri::command]
pub fn get_extension_path() -> Result<String, String> {
    let ext_dir = crate::engine::browser_integration::get_embedded_extension_dir();
    Ok(ext_dir.to_string_lossy().to_string())
}

/// Open the browser extensions page (e.g. chrome://extensions).
#[tauri::command]
pub fn launch_browser_extensions(browser: String) -> Result<(), String> {
    crate::engine::browser_integration::launch_browser_extension_page(&browser)
}

/// Check if the target partition has enough free space for the requested download.
#[tauri::command]
pub async fn check_disk_space(
    state: State<'_, AppState>,
    destination: Option<String>,
    required_bytes: u64,
) -> Result<serde_json::Value, String> {
    let dest_dir = if let Some(d) = destination {
        if !d.is_empty() {
            PathBuf::from(d)
        } else {
            state.manager.read().default_download_dir.clone()
        }
    } else {
        state.manager.read().default_download_dir.clone()
    };

    let available = storage::get_available_disk_space(&dest_dir)
        .map_err(|e| format!("Could not inspect disk space: {}", e))?;

    let is_sufficient = available >= required_bytes;
    Ok(serde_json::json!({
        "available_bytes": available,
        "required_bytes": required_bytes,
        "is_sufficient": is_sufficient
    }))
}

/// Force-update the decoupled video extractor core (yt-dlp) to the latest version,
/// make sure the FFmpeg merger is installed, and repair any leftover unmerged
/// `.fNNN.mp4` + `.fNNN.m4a` pairs in the default download folder.
#[tauri::command]
pub async fn update_extractor_core(state: State<'_, AppState>) -> Result<String, String> {
    let (client, download_dir) = {
        let mgr = state.manager.read();
        (mgr.http_client.clone(), mgr.default_download_dir.clone())
    };

    use crate::engine::extractor::MediaExtractor;

    let ytdlp_msg = MediaExtractor::update_ytdlp_core(&client).await?;

    let ffmpeg_msg = match MediaExtractor::ensure_ffmpeg_binary(&client).await {
        Ok(_) => "FFmpeg ready".to_string(),
        Err(e) => return Err(format!("{} | FFmpeg install failed: {}", ytdlp_msg, e)),
    };

    let deno_msg = match MediaExtractor::ensure_deno_binary(&client).await {
        Ok(_) => "YouTube JS runtime ready".to_string(),
        Err(e) => format!("JS runtime skipped ({})", e),
    };

    // Repair leftovers (loop: one pair merged per call)
    let mut repaired = 0;
    while repaired < 50 {
        match MediaExtractor::auto_merge_unmerged_parts(&client, &download_dir).await {
            Some(_) => repaired += 1,
            None => break,
        }
    }

    if repaired > 0 {
        Ok(format!("{} | {} | {} | Merged {} leftover file(s)", ytdlp_msg, ffmpeg_msg, deno_msg, repaired))
    } else {
        Ok(format!("{} | {} | {}", ytdlp_msg, ffmpeg_msg, deno_msg))
    }
}

/// On-demand auto-bootstrap of FFmpeg from yt-dlp/FFmpeg-Builds for high-res stream merging.
#[tauri::command]
pub async fn ensure_ffmpeg_core(state: State<'_, AppState>) -> Result<String, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    let path = crate::engine::extractor::MediaExtractor::ensure_ffmpeg_binary(&client).await?;
    Ok(format!("FFmpeg core active at: {:?}", path))
}

/// Check if FFmpeg is installed and accessible.
#[tauri::command]
pub fn check_ffmpeg_status() -> Result<bool, String> {
    Ok(crate::engine::extractor::MediaExtractor::find_ffmpeg_binary().is_some())
}

/// Update the main window title for real-time taskbar speed display.
#[tauri::command]
pub async fn set_window_title(window: tauri::Window, title: String) -> Result<(), String> {
    window.set_title(&title).map_err(|e| e.to_string())
}

/// Open an external URL in the default web browser (or Edge for store links).
#[tauri::command]
pub fn open_external_url(url: String) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("cmd")
            .args(&["/c", "start", "", &url])
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(&url)
            .spawn()
            .map_err(|e| format!("Failed to open URL: {}", e))?;
    }
    Ok(())
}

/// Probe and return real measured versions and status of yt-dlp, FFmpeg, and JS runtime.
#[tauri::command]
pub async fn get_engine_status() -> Result<crate::engine::extractor::EngineStatus, String> {
    tokio::task::spawn_blocking(crate::engine::extractor::MediaExtractor::engine_status)
        .await
        .map_err(|e| format!("Engine status probe failed: {}", e))
}

/// Check GitHub Releases for a newer version of SuperIDM desktop app.
#[tauri::command]
pub async fn check_app_update(state: State<'_, AppState>) -> Result<serde_json::Value, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };
    let current = env!("CARGO_PKG_VERSION").trim_start_matches('v').to_string();
    let req = client
        .get("https://api.github.com/repos/pengener-crypto/SuperIDM/releases/latest")
        .header("User-Agent", format!("SuperIDM-Core/{}", current))
        .header("Accept", "application/vnd.github+json")
        .send();

    let resp = tokio::time::timeout(std::time::Duration::from_secs(8), req)
        .await
        .map_err(|_| "Update server timed out".to_string())?
        .map_err(|e| format!("Update check failed: {}", e))?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned HTTP {}", resp.status()));
    }

    let json: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let latest_raw = json
        .get("tag_name")
        .and_then(|v| v.as_str())
        .unwrap_or(&current);
    let latest = latest_raw.trim().trim_start_matches('v').to_string();

    let parse_ver = |s: &str| -> (u32, u32, u32) {
        let mut parts = s.split('.').map(|p| p.parse::<u32>().unwrap_or(0));
        (
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
            parts.next().unwrap_or(0),
        )
    };
    let update_available = parse_ver(&latest) > parse_ver(&current);

    let mut setup_url = format!(
        "https://github.com/pengener-crypto/SuperIDM/releases/download/v{}/SuperIDM_Setup.exe",
        latest
    );
    if let Some(assets) = json.get("assets").and_then(|v| v.as_array()) {
        for a in assets {
            if a.get("name").and_then(|n| n.as_str()) == Some("SuperIDM_Setup.exe") {
                if let Some(dl) = a.get("browser_download_url").and_then(|u| u.as_str()) {
                    setup_url = dl.to_string();
                }
            }
        }
    }

    Ok(serde_json::json!({
        "current_version": current,
        "latest_version": latest,
        "update_available": update_available,
        "download_url": setup_url,
        "release_url": json.get("html_url").and_then(|u| u.as_str()).unwrap_or("https://github.com/pengener-crypto/SuperIDM/releases/latest"),
    }))
}

/// Generate a complete diagnostic report (engine binaries, versions, tasks, backend & UI logs)
/// and save it to Desktop\SuperIDM_Diagnostic_Report.txt.
#[tauri::command]
pub async fn export_diagnostic_report(
    state: State<'_, AppState>,
    ui_logs: Option<Vec<String>>,
    #[allow(non_snake_case)] uiLogs: Option<Vec<String>>,
) -> Result<serde_json::Value, String> {
    let status = tokio::task::spawn_blocking(crate::engine::extractor::MediaExtractor::engine_status)
        .await
        .map_err(|e| format!("Engine probe failed: {}", e))?;

    let (tasks_snapshot, download_dir, active_tasks, max_concurrent, throttler) = {
        let mgr = state.manager.read();
        let list: Vec<String> = mgr
            .tasks
            .values()
            .map(|h| {
                let m = &h.metadata;
                format!(
                    "  - [{}] {:?} | {} | {} / {} ({}) | conns={} | url={}",
                    &m.id[..m.id.len().min(8)],
                    m.status,
                    m.filename,
                    crate::engine::speed::format_bytes(m.downloaded_bytes()),
                    crate::engine::speed::format_bytes(m.total_bytes),
                    h.speed_meter.speed_formatted(),
                    m.num_connections,
                    m.url
                )
            })
            .collect();
        (
            list,
            mgr.default_download_dir.clone(),
            mgr.active_count(),
            mgr.max_concurrent,
            mgr.throttler.clone(),
        )
    };

    let limit_bps = throttler.get_limit().await;
    let limit_str = if limit_bps == 0 {
        "Unlimited".to_string()
    } else {
        crate::engine::speed::format_bytes_per_sec(limit_bps as f64)
    };

    let backend_logs = crate::engine::extractor::recent_engine_log();
    let frontend_logs = ui_logs.or(uiLogs).unwrap_or_default();

    let mut report = String::new();
    report.push_str("============================================================\n");
    report.push_str(&format!(
        "SuperIDM Diagnostic Report — v{}\n",
        env!("CARGO_PKG_VERSION")
    ));
    report.push_str(&format!(
        "Generated At: {}\n",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S %z")
    ));
    report.push_str(&format!(
        "OS / Arch:    {} / {}\n",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    report.push_str(&format!(
        "Download Dir: {}\n",
        download_dir.to_string_lossy()
    ));
    report.push_str(&format!(
        "Concurrency:  {} active / {} max | Throttle: {}\n",
        active_tasks, max_concurrent, limit_str
    ));
    report.push_str("============================================================\n\n");

    report.push_str("[MEDIA ENGINE COMPONENTS]\n");
    report.push_str(&format!("  Overall Ready : {}\n", status.ready));
    report.push_str(&format!(
        "  yt-dlp Path   : {}\n",
        status.ytdlp_path.as_deref().unwrap_or("NOT FOUND")
    ));
    report.push_str(&format!(
        "  yt-dlp Ver    : {}\n",
        status.ytdlp_version.as_deref().unwrap_or("INVALID / MISSING")
    ));
    report.push_str(&format!(
        "  FFmpeg Path   : {}\n",
        status.ffmpeg_path.as_deref().unwrap_or("NOT FOUND")
    ));
    report.push_str(&format!("  FFmpeg OK     : {}\n", status.ffmpeg_ok));
    report.push_str(&format!(
        "  JS Runtime    : {}\n",
        status.js_runtime.as_deref().unwrap_or("NOT FOUND")
    ));
    report.push_str(&format!("  JS Runtime OK : {}\n\n", status.js_ok));

    report.push_str(&format!("[TASKS ({})]\n", tasks_snapshot.len()));
    if tasks_snapshot.is_empty() {
        report.push_str("  (no tasks)\n\n");
    } else {
        for t in &tasks_snapshot {
            report.push_str(t);
            report.push('\n');
        }
        report.push('\n');
    }

    report.push_str(&format!("[BACKEND ENGINE LOGS ({})]\n", backend_logs.len()));
    if backend_logs.is_empty() {
        report.push_str("  (no backend errors recorded)\n\n");
    } else {
        for l in &backend_logs {
            report.push_str("  ");
            report.push_str(l);
            report.push('\n');
        }
        report.push('\n');
    }

    report.push_str(&format!("[UI ACTIVITY LOGS ({})]\n", frontend_logs.len()));
    for l in frontend_logs.iter().take(60) {
        report.push_str("  ");
        report.push_str(l);
        report.push('\n');
    }

    let out_dir = dirs::desktop_dir().unwrap_or(download_dir);
    let out_path = out_dir.join("SuperIDM_Diagnostic_Report.txt");
    let _ = tokio::fs::write(&out_path, report.as_bytes()).await;

    Ok(serde_json::json!({
        "path": out_path.to_string_lossy().to_string(),
        "report": report,
        "status": status,
    }))
}

/// Return the authoritative compiled application version from Cargo.toml.
#[tauri::command]
pub fn get_app_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Update maximum simultaneous downloads tracked by the backend manager.
#[tauri::command]
pub fn set_max_concurrent(
    state: State<'_, AppState>,
    max_concurrent: Option<usize>,
    #[allow(non_snake_case)] maxConcurrent: Option<usize>,
) -> usize {
    let clamped = max_concurrent.or(maxConcurrent).unwrap_or(MAX_CONCURRENT_DOWNLOADS).clamp(1, 16);
    let mut mgr = state.manager.write();
    mgr.max_concurrent = clamped;
    clamped
}

/// Schedule a safe OS shutdown after a grace countdown (clamped to 10..=600 seconds, or 0 for immediate).
#[tauri::command]
pub fn schedule_system_shutdown(
    delay_secs: Option<u64>,
    #[allow(non_snake_case)] delaySecs: Option<u64>,
) -> Result<u64, String> {
    let delay = delay_secs.or(delaySecs).unwrap_or(60).min(600);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        // Abort any previously queued shutdown before scheduling a fresh countdown.
        let _ = std::process::Command::new("shutdown")
            .args(["/a"])
            .creation_flags(0x08000000)
            .output();
        let output = std::process::Command::new("shutdown")
            .args([
                "/s",
                "/t",
                &delay.to_string(),
                "/c",
                "SuperIDM: All downloads completed. Shutting down PC.",
            ])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to invoke Windows shutdown: {}", e))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() { stderr } else { stdout };
            return Err(format!("Windows shutdown command failed: {}", detail));
        }
    }
    #[cfg(not(target_os = "windows"))]
    {
        let mins = ((delay + 59) / 60).max(1);
        let status = std::process::Command::new("shutdown")
            .args(["-h", &format!("+{}", mins)])
            .status()
            .map_err(|e| format!("Failed to schedule shutdown: {}", e))?;
        if !status.success() {
            return Err("System shutdown command exited with an error".to_string());
        }
    }
    Ok(delay)
}

/// Cancel a previously scheduled OS shutdown (`shutdown /a` on Windows).
#[tauri::command]
pub fn cancel_system_shutdown() -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        let _ = std::process::Command::new("shutdown")
            .args(["/a"])
            .creation_flags(0x08000000)
            .output()
            .map_err(|e| format!("Failed to cancel Windows shutdown: {}", e))?;
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = std::process::Command::new("shutdown")
            .args(["-c"])
            .output();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_resolve_candidate_paths_exact_dir_and_sanitized() {
        let dir = TempDir::new().unwrap();
        let video_real = dir.path().join("Is Google Gemini Back？.mp4");
        std::fs::write(&video_real, b"video bytes").unwrap();

        // When destination is just the folder and UI sanitized `?` -> `_`
        let cands = resolve_candidate_file_paths(
            Some(&dir.path().to_string_lossy()),
            Some("Is Google Gemini Back_.mp4"),
            dir.path(),
        );
        assert!(cands.iter().any(|p| p == &video_real && p.is_file()));
    }

    #[tokio::test]
    async fn test_cleanup_task_files_deletes_video_and_parts() {
        let dir = TempDir::new().unwrap();
        let main_file = dir.path().join("My Video.mp4");
        let part_file = dir.path().join("My Video.mp4.part");
        let frag_file = dir.path().join("My Video.f399.mp4.part");
        let sidecar_file = PathBuf::from(format!("{}.part.json", main_file.to_string_lossy()));

        std::fs::write(&main_file, b"mp4").unwrap();
        std::fs::write(&part_file, b"part").unwrap();
        std::fs::write(&frag_file, b"frag").unwrap();
        std::fs::write(&sidecar_file, b"{}").unwrap();

        // delete_completed_file = false removes only sidecar/temp fragments, preserves main file
        let _ = cleanup_task_files(&[main_file.clone()], false).await;
        assert!(main_file.exists());
        assert!(!part_file.exists());
        assert!(!frag_file.exists());
        assert!(!sidecar_file.exists());

        // delete_completed_file = true physically deletes the video/program itself
        let deleted = cleanup_task_files(&[main_file.clone()], true).await;
        assert!(!main_file.exists());
        assert!(!deleted.is_empty());
    }
}


