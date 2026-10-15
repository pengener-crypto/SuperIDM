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

    let raw_url = req_obj
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'url' parameter in download request".to_string())?;
    let url = crate::engine::extractor::MediaExtractor::clean_target_url(raw_url);

    let destination = req_obj
        .get("destination")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    let filename = req_obj
        .get("filename")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let format_id = req_obj
        .get("format")
        .or_else(|| req_obj.get("format_id"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let num_connections = req_obj
        .get("numConnections")
        .or_else(|| req_obj.get("num_connections"))
        .and_then(|v| {
            v.as_u64()
                .map(|n| n as usize)
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(DEFAULT_CONNECTIONS);

    let priority = req_obj
        .get("priority")
        .and_then(|v| {
            v.as_u64()
                .map(|n| n as u8)
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
        })
        .unwrap_or(0);

    let category = req_obj
        .get("category")
        .and_then(|v| v.as_str())
        .unwrap_or("General")
        .to_string();

    let headers: Option<Vec<(String, String)>> = req_obj.get("headers").and_then(|h| {
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

        let metadata = DownloadMetadata {
            id: task_id.clone(),
            url: url.clone(),
            destination: file_path.to_string_lossy().to_string(),
            filename: initial_name,
            total_bytes: 50 * 1024 * 1024,
            status: TaskStatus::Downloading,
            etag: None,
            last_modified: None,
            supports_range: true,
            num_connections: 8,
            priority,
            category: "Videos".to_string(),
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

        let client_c = state.manager.read().http_client.clone();
        let url_c = url.clone();
        let dest_dir_ext = dest_dir.clone();
        let dest_dir_watch = dest_dir.clone();
        let cancel_c = cancel_rx.clone();
        let format_id_c = format_id.clone();

        // Spawn platform stream extractor
        tokio::spawn(async move {
            let res = crate::engine::extractor::MediaExtractor::download_media_stream(
                &client_c,
                &url_c,
                &dest_dir_ext,
                format_id_c.as_deref(),
                prog_tx,
                cancel_c,
            ).await;
            if let Err(e) = res {
                eprintln!("[MediaExtractor Error] {}", e);
            }
        });

        // Watch progress
        let app_handle = app.clone();
        let task_id_c = task_id.clone();
        let mgr_arc = state.manager.clone();

        tokio::spawn(async move {
            let mut completed = false;
            while let Some((progress, speed, downloaded, total, eta_secs, is_comp, final_name_opt)) = prog_rx.recv().await {
                {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        let eff_total = if total > 0 { total } else { 100_000 };
                        let eff_downloaded = if downloaded > 0 { downloaded } else { (progress * 100_000.0) as u64 };
                        h.metadata.total_bytes = eff_total;
                        if let Some(ref fname) = final_name_opt {
                            h.metadata.filename = fname.clone();
                            let new_dest = dest_dir_watch.join(fname);
                            h.metadata.destination = new_dest.to_string_lossy().to_string();
                        }
                        if h.metadata.chunks.is_empty() {
                            h.metadata.chunks.push(ChunkInfo {
                                index: 0,
                                start_byte: 0,
                                end_byte: eff_total,
                                current_byte: eff_downloaded,
                                completed: is_comp,
                                retry_count: 0,
                            });
                        } else {
                            h.metadata.chunks[0].start_byte = 0;
                            h.metadata.chunks[0].current_byte = eff_downloaded;
                            h.metadata.chunks[0].end_byte = eff_total;
                            h.metadata.chunks[0].completed = is_comp;
                        }
                        if is_comp {
                            h.metadata.status = TaskStatus::Completed;
                            completed = true;
                        }
                    }
                }

                let eff_total = if total > 0 { total } else { 100_000 };
                let eff_downloaded = if downloaded > 0 { downloaded } else { (progress * 100_000.0) as u64 };

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
                    break;
                }
            }

            if !completed {
                let mut mgr = mgr_arc.write();
                if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                    if h.metadata.status != TaskStatus::Completed && h.metadata.status != TaskStatus::Paused {
                        h.metadata.status = TaskStatus::Failed;
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
                        let _ = app_handle.emit("download_progress", &payload);
                    }
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

    let total_bytes = if probe.content_length > 0 {
        probe.content_length
    } else {
        50 * 1024 * 1024 // Fallback 50MB estimate if unknown
    };

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

    let chunks = chunker::calculate_chunks(total_bytes, effective_conns);

    // Pre-allocate file
    let _ = storage::preallocate_file(&file_path, total_bytes).await;

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
        let _ = storage::save_part_json_atomic(&fp, &meta).await;
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
    if crate::engine::extractor::MediaExtractor::is_media_url(&metadata.url) || metadata.chunks.is_empty() {
        let dest_dir = PathBuf::from(&metadata.destination)
            .parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        let (prog_tx, mut prog_rx) = mpsc::channel(128);
        let url_c = metadata.url.clone();
        let dest_dir_ext = dest_dir.clone();
        let dest_dir_watch = dest_dir.clone();
        let cancel_c = cancel_rx.clone();

        let client_c = state.manager.read().http_client.clone();
        tokio::spawn(async move {
            let _ = crate::engine::extractor::MediaExtractor::download_media_stream(
                &client_c,
                &url_c,
                &dest_dir_ext,
                None,
                prog_tx,
                cancel_c,
            ).await;
        });

        let app_handle = app.clone();
        let task_id_c = id.clone();
        let mgr_arc = state.manager.clone();

        tokio::spawn(async move {
            while let Some((progress, speed, downloaded, total, eta_secs, is_comp, final_name_opt)) = prog_rx.recv().await {
                {
                    let mut mgr = mgr_arc.write();
                    if let Some(h) = mgr.tasks.get_mut(&task_id_c) {
                        h.metadata.total_bytes = total;
                        if let Some(ref fname) = final_name_opt {
                            h.metadata.filename = fname.clone();
                            let new_dest = dest_dir_watch.join(fname);
                            h.metadata.destination = new_dest.to_string_lossy().to_string();
                        }
                        if h.metadata.chunks.is_empty() {
                            h.metadata.chunks.push(ChunkInfo {
                                index: 0,
                                start_byte: 0,
                                end_byte: total,
                                current_byte: downloaded,
                                completed: is_comp,
                                retry_count: 0,
                            });
                        } else {
                            h.metadata.chunks[0].current_byte = downloaded;
                            h.metadata.chunks[0].end_byte = total;
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
                    downloaded_bytes: downloaded,
                    total_bytes: total,
                    progress,
                    speed: if is_comp { 0.0 } else { speed },
                    chunks: vec![],
                    eta_secs: if is_comp { 0.0 } else { eta_secs },
                };

                let _ = app_handle.emit("download_progress", &payload);

                if is_comp {
                    break;
                }
            }
        });

        return Ok(());
    }

    // Direct multi-chunk resume with work stealing support
    let file_path = PathBuf::from(&metadata.destination);
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>(4096);
    let chunk_limits: downloader::ChunkLimitMap = Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new()));

    for chunk in &metadata.chunks {
        if chunk.completed {
            continue;
        }
        let client_c = client.clone();
        let url_c = metadata.url.clone();
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

/// Cancel a download task permanently.
#[tauri::command]
pub async fn cancel_download(
    state: State<'_, AppState>,
    task_id: Option<String>,
    #[allow(non_snake_case)]
    taskId: Option<String>,
) -> Result<(), String> {
    let id = task_id.or(taskId).ok_or_else(|| "Missing task_id".to_string())?;
    let path = {
        let mut mgr = state.manager.write();
        if let Some(handle) = mgr.tasks.remove(&id) {
            let _ = handle.cancel_token.send(true);
            Some(PathBuf::from(&handle.metadata.destination))
        } else {
            None
        }
    };

    if let Some(path) = path {
        let _ = storage::delete_part_json(&path).await;
        let _ = tokio::fs::remove_file(&path).await;
    }

    let all_tasks: Vec<_> = state.manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
    let _ = storage::save_tasks_db(&all_tasks).await;

    Ok(())
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
                progress: if tot > 0 {
                    meta.progress()
                } else if !meta.chunks.is_empty() && meta.chunks[0].end_byte > 0 {
                    meta.chunks[0].downloaded() as f64 / meta.chunks[0].end_byte as f64
                } else {
                    0.0
                },
                speed: spd,
                status: meta.status.clone(),
                category: meta.category.clone(),
                priority: meta.priority,
                num_connections: meta.num_connections,
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

/// Open a folder in Windows Explorer.
#[tauri::command]
pub async fn open_download_folder(
    path: Option<String>,
    #[allow(non_snake_case)]
    folderPath: Option<String>,
) -> Result<(), String> {
    let target = path.or(folderPath).unwrap_or_else(|| "".to_string()).trim().to_string();
    let default_downloads = dirs::download_dir().unwrap_or_else(|| PathBuf::from("C:\\Users\\Default\\Downloads"));

    let target_path = if target.is_empty() {
        default_downloads.clone()
    } else {
        PathBuf::from(&target)
    };

    #[cfg(target_os = "windows")]
    {
        if target_path.is_file() || (target_path.exists() && target_path.extension().is_some()) {
            let path_str = target_path.to_string_lossy().to_string();
            let _ = std::process::Command::new("explorer.exe")
                .args(&["/select,", &path_str])
                .spawn();
        } else if target_path.exists() && target_path.is_dir() {
            let _ = std::process::Command::new("explorer.exe")
                .arg(&target_path)
                .spawn();
        } else if let Some(parent) = target_path.parent() {
            // Never fall back to a bare drive root like "C:" — use Downloads instead.
            let parent_is_drive_root = parent.parent().is_none();
            if parent.is_dir() && !parent_is_drive_root && target_path.is_absolute() {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(parent)
                    .spawn();
            } else {
                let _ = std::process::Command::new("explorer.exe")
                    .arg(&default_downloads)
                    .spawn();
            }
        } else {
            let _ = std::process::Command::new("explorer.exe")
                .arg(&default_downloads)
                .spawn();
        }
    }

    Ok(())
}

/// Open a downloaded file with default Windows associated application.
#[tauri::command]
pub async fn open_file(
    path: Option<String>,
    #[allow(non_snake_case)]
    filePath: Option<String>,
) -> Result<(), String> {
    let target = path.or(filePath).unwrap_or_else(|| "".to_string()).trim().to_string();
    if target.is_empty() {
        return Err("No file path provided".to_string());
    }
    let target_path = PathBuf::from(&target);
    if !target_path.exists() {
        return Err(format!("File does not exist: {}", target));
    }

    #[cfg(target_os = "windows")]
    {
        let path_str = target_path.to_string_lossy().to_string();
        let _ = std::process::Command::new("cmd")
            .args(&["/c", "start", "", &path_str])
            .spawn();
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = open::that(&target_path);
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
        total_bytes: 100 * 1024 * 1024,
        status: TaskStatus::Downloading,
        etag: None,
        last_modified: None,
        supports_range: true,
        num_connections: 8,
        priority: 1,
        category,
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

    let fresh_url = if let Some(ref u) = provided_new {
        let trimmed = u.trim();
        if !trimmed.is_empty() {
            trimmed.to_string()
        } else {
            let (source_url, mut placeholder) = {
                let mgr = state.manager.read();
                let h = mgr.tasks.get(&id).ok_or_else(|| "Task not found".to_string())?;
                let s_url = provided_orig.unwrap_or_else(|| h.metadata.url.clone());
                (s_url, h.metadata.clone())
            };
            crate::engine::recovery::RecoveryEngine::refresh_expired_link(
                &client,
                &source_url,
                &mut placeholder,
            ).await?
        }
    } else {
        let (source_url, mut placeholder) = {
            let mgr = state.manager.read();
            let h = mgr.tasks.get(&id).ok_or_else(|| "Task not found".to_string())?;
            let s_url = provided_orig.unwrap_or_else(|| h.metadata.url.clone());
            (s_url, h.metadata.clone())
        };
        crate::engine::recovery::RecoveryEngine::refresh_expired_link(
            &client,
            &source_url,
            &mut placeholder,
        ).await?
    };

    let (dest_path, meta_clone, all_tasks) = {
        let mut mgr = state.manager.write();
        let h = mgr.tasks.get_mut(&id).ok_or_else(|| "Task not found".to_string())?;
        h.metadata.url = fresh_url.clone();
        h.metadata.updated_at = chrono::Utc::now().timestamp_millis();
        if h.metadata.status == TaskStatus::Failed {
            h.metadata.status = TaskStatus::Paused;
        }
        let dest = PathBuf::from(&h.metadata.destination);
        let meta = h.metadata.clone();
        let all = mgr.tasks.values().map(|t| t.metadata.clone()).collect::<Vec<_>>();
        (dest, meta, all)
    };

    // Save updated checkpoint .part.json with fresh URL
    let _ = storage::save_part_json_atomic(&dest_path, &meta_clone).await;
    let _ = storage::save_tasks_db(&all_tasks).await;

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
    let desktop_dir = dirs::desktop_dir().map(|d| d.join("SuperIDM-Extension"));
    let appdata_dir = crate::engine::browser_integration::get_embedded_extension_dir();

    let target_dir = if let Some(ref d) = desktop_dir {
        if d.exists() {
            d.clone()
        } else {
            appdata_dir
        }
    } else {
        appdata_dir
    };

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

/// Force-update the decoupled video extractor core (yt-dlp) to the latest version.
#[tauri::command]
pub async fn update_extractor_core(state: State<'_, AppState>) -> Result<String, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    crate::engine::extractor::MediaExtractor::update_ytdlp_core(&client).await
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


