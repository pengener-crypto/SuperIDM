use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::RwLock;
use tauri::{AppHandle, Emitter, State};
use tokio::sync::{mpsc, watch};
use uuid::Uuid;

use crate::engine::chunker;
use crate::engine::downloader::{self, DownloadManager, TaskHandle, WorkerEvent};
use crate::engine::speed::SpeedMeter;
use crate::engine::storage;
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

    let url = req_obj
        .get("url")
        .and_then(|v| v.as_str())
        .ok_or_else(|| "Missing 'url' parameter in download request".to_string())?
        .to_string();

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

    // Check if this is a video platform URL that requires signature/token extraction (YouTube, TikTok, Vimeo, etc.)
    if crate::engine::extractor::MediaExtractor::is_platform_url(&url) {
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
                            completed = true;
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
        !f_trim.is_empty() 
            && f_trim != "download" 
            && f_trim != "download.dat"
            && f_trim != "video" 
            && f_trim != "video.mp4"
            && !f_trim.starts_with("download_")
    });

    let filename = if let Some(exp) = explicit_name {
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

    let file_path = dest_dir.join(&filename);

    let total_bytes = if probe.content_length > 0 {
        probe.content_length
    } else {
        50 * 1024 * 1024 // Fallback 50MB estimate if unknown
    };

    let effective_conns = if probe.supports_range && probe.content_length > 0 {
        num_connections.clamp(1, 32)
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

    // Spawn chunk workers with shared event channel
    let throttler = state.manager.read().throttler.clone();
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>(512);

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
            )
            .await;
        });
    }

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

/// Download Coordinator Loop that processes worker events, updates metadata,
/// smooths speed, and notifies the frontend.
async fn run_coordinator(
    manager: Arc<RwLock<DownloadManager>>,
    app: AppHandle,
    task_id: String,
    file_path: PathBuf,
    mut event_rx: mpsc::Receiver<WorkerEvent>,
) {
    let mut last_emit = tokio::time::Instant::now();

    while let Some(event) = event_rx.recv().await {
        let (is_completed, payload) = {
            let mut mgr = manager.write();
            if let Some(handle) = mgr.tasks.get_mut(&task_id) {
                match event {
                    WorkerEvent::Progress {
                        chunk_index,
                        bytes_written,
                        new_offset,
                    } => {
                        if chunk_index < handle.metadata.chunks.len() {
                            handle.metadata.chunks[chunk_index].current_byte = new_offset;
                        }
                        handle.speed_meter.record(bytes_written);
                    }
                    WorkerEvent::ChunkDone { chunk_index } => {
                        if chunk_index < handle.metadata.chunks.len() {
                            handle.metadata.chunks[chunk_index].completed = true;
                            handle.metadata.chunks[chunk_index].current_byte =
                                handle.metadata.chunks[chunk_index].end_byte + 1;
                        }
                    }
                    WorkerEvent::ChunkError { chunk_index, error, will_retry } => {
                        eprintln!("[SuperIDM] Chunk {} error: {} (retry: {})", chunk_index, error, will_retry);
                    }
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
                let all_tasks: Vec<_> = manager.read().tasks.values().map(|h| h.metadata.clone()).collect();
                let _ = storage::save_tasks_db(&all_tasks).await;
                break;
            } else if last_emit.elapsed() >= Duration::from_millis(150) {
                let _ = app.emit("download_progress", &pl);
                last_emit = tokio::time::Instant::now();
            }
        }
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

    // Direct multi-chunk resume
    let file_path = PathBuf::from(&metadata.destination);
    let (event_tx, event_rx) = mpsc::channel::<WorkerEvent>(512);

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
            )
            .await;
        });
    }

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
                progress: meta.progress(),
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
            if parent.exists() {
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
                eta_secs: if spd > 0.0 && progress < 1.0 { ((1.0 - progress) * total_bytes as f64 / progress) / spd } else { 0.0 },
            };

            let _ = app_handle.emit("download_progress", &payload);

            if is_comp {
                break;
            }
        }
    });

    Ok(task_id)
}

/// Refresh an expired temporary signed URL using the original source link.
#[tauri::command]
pub async fn refresh_task_link(
    state: State<'_, AppState>,
    task_id: String,
    original_url: String,
) -> Result<String, String> {
    let client = {
        let mgr = state.manager.read();
        mgr.http_client.clone()
    };

    let mut placeholder_meta = DownloadMetadata {
        id: task_id.clone(),
        url: original_url.clone(),
        destination: String::new(),
        filename: String::new(),
        total_bytes: 0,
        status: TaskStatus::Downloading,
        etag: None,
        last_modified: None,
        supports_range: true,
        num_connections: 8,
        priority: 1,
        category: "General".into(),
        chunks: Vec::new(),
        created_at: 0,
        updated_at: 0,
    };

    let fresh_url = crate::engine::recovery::RecoveryEngine::refresh_expired_link(
        &client,
        &original_url,
        &mut placeholder_meta,
    ).await?;

    let mut mgr = state.manager.write();
    if let Some(h) = mgr.tasks.get_mut(&task_id) {
        h.metadata.url = fresh_url.clone();
    }

    Ok(fresh_url)
}

/// Open the extension folder in Windows Explorer so the user can load it manually.
#[tauri::command]
pub async fn open_extension_folder() -> Result<(), String> {
    let exe_path = std::env::current_exe().map_err(|e| format!("Cannot find exe: {}", e))?;
    let base_dir = exe_path.parent().ok_or("Cannot find parent dir")?;
    let ext_dir = base_dir.join("extension");

    // If the extension folder doesn't exist next to the exe, check the workspace root
    let ext_dir = if ext_dir.exists() {
        ext_dir
    } else {
        // Fallback: check relative to the cargo manifest
        let fallback = PathBuf::from(env!("CARGO_MANIFEST_DIR")).parent()
            .map(|p| p.join("extension"))
            .unwrap_or(ext_dir);
        fallback
    };

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(&ext_dir)
            .spawn()
            .map_err(|e| format!("Failed to open extension folder: {}", e))?;
    }

    Ok(())
}

/// Poll for pending RPC download requests from the browser extension.
/// The frontend calls this periodically and auto-starts any queued downloads.
#[tauri::command]
pub fn poll_rpc_downloads(
    state: State<'_, AppState>,
) -> Vec<serde_json::Value> {
    let mut mgr = state.manager.write();
    let pending: Vec<_> = mgr.pending_rpc_downloads.drain(..).collect();
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

