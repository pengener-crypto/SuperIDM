mod commands;
mod engine;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use commands::AppState;
use engine::downloader::DownloadManager;
use engine::rpc::RpcServer;
use engine::storage;
use parking_lot::RwLock;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

/// Shared flag: when `true`, the app is allowed to fully exit.
static SHOULD_EXIT: AtomicBool = AtomicBool::new(false);
/// Shared preference synced from Settings: minimize window to system tray on close.
static MINIMIZE_TO_TRAY: AtomicBool = AtomicBool::new(true);

pub fn set_minimize_to_tray_flag(enabled: bool) {
    MINIMIZE_TO_TRAY.store(enabled, Ordering::SeqCst);
}

pub fn get_minimize_to_tray_flag() -> bool {
    MINIMIZE_TO_TRAY.load(Ordering::SeqCst)
}

/// Run the SuperIDM Tauri application.
///
/// Sets up the download manager state, scans for interrupted downloads
/// (`.part.json` sidecars), starts the RPC server, and launches the
/// Tauri window with the cyber Fluent UI and system tray background engine.
#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Determine default download directory
    let download_dir = dirs::download_dir().unwrap_or_else(|| {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Downloads")
    });

    // Initialize download manager
    let manager = Arc::new(RwLock::new(DownloadManager::new(download_dir.clone())));

    let app_state = AppState {
        manager: manager.clone(),
    };

fn bring_window_to_front<R: tauri::Runtime>(window: &tauri::WebviewWindow<R>) {
    let _ = window.show();
    let _ = window.unminimize();
    let _ = window.set_always_on_top(true);
    let _ = window.set_focus();
    let _ = window.set_always_on_top(false);
}

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                bring_window_to_front(&window);
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_default_save_path,
            commands::set_default_save_path,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::get_all_tasks,
            commands::set_global_throttle,
            commands::open_download_folder,
            commands::open_file,
            commands::extract_media_info,
            commands::extract_playlist,
            commands::start_stream_download,
            commands::refresh_task_link,
            commands::notify_download_complete,
            commands::open_extension_folder,
            commands::poll_rpc_downloads,
            commands::get_rpc_token,
            commands::install_browser_integration,
            commands::get_extension_path,
            commands::launch_browser_extensions,
            commands::check_disk_space,
            commands::update_extractor_core,
            commands::ensure_ffmpeg_core,
            commands::check_ffmpeg_status,
            commands::set_window_title,
            commands::open_external_url,
            commands::focus_window,
            commands::get_engine_status,
            commands::check_app_update,
            commands::export_diagnostic_report,
            commands::get_app_version,
            commands::set_max_concurrent,
            commands::set_max_retries,
            commands::set_minimize_to_tray,
            commands::set_auto_extract,
            commands::set_show_notifications,
            commands::set_browser_rules,
            commands::extract_archive,
            commands::schedule_system_shutdown,
            commands::cancel_system_shutdown,
        ])
        .setup(move |app| {
            let manager_clone = manager.clone();
            let download_dir_clone = download_dir.clone();

            // Auto-extract and sync embedded extension files on startup
            let rpc_token = engine::rpc::get_or_create_rpc_token();
            let _ = engine::browser_integration::extract_and_sync_extension(&rpc_token);

            // Ensure main window is displayed and focused on launch
            if let Some(window) = app.get_webview_window("main") {
                bring_window_to_front(&window);
            }
            let app_h = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                if let Some(window) = app_h.get_webview_window("main") {
                    bring_window_to_front(&window);
                }
            });

            // Configure System Tray
            let show_i = MenuItem::with_id(app, "show", "Show SuperIDM", true, None::<&str>)?;
            let add_i = MenuItem::with_id(app, "add", "Add New Download...", true, None::<&str>)?;
            let pause_all_i = MenuItem::with_id(app, "pause_all", "Pause All", true, None::<&str>)?;
            let resume_all_i = MenuItem::with_id(app, "resume_all", "Resume All", true, None::<&str>)?;
            let sep = PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Exit SuperIDM", true, None::<&str>)?;

            let tray_menu = Menu::with_items(app, &[&show_i, &add_i, &pause_all_i, &resume_all_i, &sep, &quit_i])?;

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().cloned().unwrap())
                .menu(&tray_menu)
                .tooltip("SuperIDM Download Manager (Running in background)")
                .on_menu_event(|app, event| {
                    match event.id.as_ref() {
                        "show" => {
                            if let Some(window) = app.get_webview_window("main") {
                                bring_window_to_front(&window);
                            }
                        }
                        "add" => {
                            if let Some(window) = app.get_webview_window("main") {
                                bring_window_to_front(&window);
                                let _ = window.emit("open-add-modal", ());
                            }
                        }
                        "pause_all" => {
                            let _ = app.emit("tray-pause-all", ());
                        }
                        "resume_all" => {
                            let _ = app.emit("tray-resume-all", ());
                        }
                        "quit" => {
                            // Signal that this is an intentional user-initiated exit
                            SHOULD_EXIT.store(true, Ordering::SeqCst);
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                bring_window_to_front(&window);
                            }
                        }
                    }
                })
                .build(app)?;

            // Start RPC server immediately so browser extension connects without delay
            let rpc = RpcServer::new();
            let mgr_rpc = manager_clone.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(e) = rpc.start(mgr_rpc).await {
                    eprintln!("Warning: RPC server failed to start: {}", e);
                }
            });

            // Silent daily yt-dlp background check (runs 6s after startup, at most once every 24h)
            let mgr_bg = manager_clone.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_secs(6)).await;
                let stamp_path = dirs::data_local_dir()
                    .map(|d| d.join("SuperIDM").join("bin").join("last_ytdlp_check.txt"));
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                let should_check = match stamp_path.as_ref().and_then(|p| std::fs::read_to_string(p).ok()) {
                    Some(s) => {
                        let prev = s.trim().parse::<u64>().unwrap_or(0);
                        now_secs.saturating_sub(prev) >= 86_400
                    }
                    None => true,
                };
                if should_check {
                    let client = mgr_bg.read().http_client.clone();
                    if let Ok(msg) = engine::extractor::MediaExtractor::update_ytdlp_core(&client).await {
                        engine::extractor::record_engine_log(format!("[daily-check] {}", msg));
                        if let Some(p) = stamp_path {
                            if let Some(parent) = p.parent() {
                                let _ = tokio::fs::create_dir_all(parent).await;
                            }
                            let _ = tokio::fs::write(&p, now_secs.to_string()).await;
                        }
                    }
                }
            });

            // Scan for persistent tasks db and interrupted downloads on startup
            tauri::async_runtime::spawn(async move {
                // 1. Load saved tasks from database
                if let Ok(db_tasks) = storage::load_tasks_db().await {
                    let mut mgr = manager_clone.write();
                    for meta in db_tasks {
                        let (cancel_tx, _) = tokio::sync::watch::channel(false);
                        mgr.tasks.insert(
                            meta.id.clone(),
                            engine::downloader::TaskHandle {
                                metadata: meta,
                                speed_meter: engine::speed::SpeedMeter::new(),
                                cancel_token: cancel_tx,
                            },
                        );
                    }
                }

                // 2. Scan for any interrupted .part files
                match storage::scan_for_part_files(&download_dir_clone).await {
                    Ok(recovered) => {
                        if !recovered.is_empty() {
                            let mut mgr = manager_clone.write();
                            for meta in recovered {
                                let (cancel_tx, _) = tokio::sync::watch::channel(false);
                                mgr.tasks.insert(
                                    meta.id.clone(),
                                    engine::downloader::TaskHandle {
                                        metadata: meta,
                                        speed_meter: engine::speed::SpeedMeter::new(),
                                        cancel_token: cancel_tx,
                                    },
                                );
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("Warning: Failed to scan for interrupted downloads: {}", e);
                    }
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                if MINIMIZE_TO_TRAY.load(Ordering::SeqCst) {
                    api.prevent_close();
                    let _ = window.hide();
                } else {
                    SHOULD_EXIT.store(true, Ordering::SeqCst);
                }
            }
        })
        .build(tauri::generate_context!())
        .expect("Error building SuperIDM")
        .run(|_app_handle, event| {
            if let tauri::RunEvent::ExitRequested { api, .. } = &event {
                // Only prevent exit when SHOULD_EXIT is false (window-close).
                // When the user picks "Exit SuperIDM" from the tray, SHOULD_EXIT
                // is set to true and we let the process terminate.
                if !SHOULD_EXIT.load(Ordering::SeqCst) {
                    api.prevent_exit();
                }
            }
        });
}
