mod commands;
mod engine;

use std::path::PathBuf;
use std::sync::Arc;

use commands::AppState;
use engine::downloader::DownloadManager;
use engine::rpc::RpcServer;
use engine::storage;
use parking_lot::RwLock;

use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use tauri::{Emitter, Manager, WindowEvent};

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

    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            commands::get_default_save_path,
            commands::start_download,
            commands::pause_download,
            commands::resume_download,
            commands::cancel_download,
            commands::get_all_tasks,
            commands::set_global_throttle,
            commands::open_download_folder,
            commands::extract_media_info,
            commands::start_stream_download,
            commands::refresh_task_link,
            commands::open_extension_folder,
            commands::poll_rpc_downloads,
            commands::get_rpc_token,
            commands::install_browser_integration,
            commands::get_extension_path,
            commands::launch_browser_extensions,
        ])
        .setup(move |app| {
            let manager_clone = manager.clone();
            let download_dir_clone = download_dir.clone();

            // Auto-extract and sync embedded extension files on startup
            let rpc_token = engine::rpc::get_or_create_rpc_token();
            let _ = engine::browser_integration::extract_and_sync_extension(&rpc_token);

            // Ensure main window is displayed and focused on launch
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }

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
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                        "add" => {
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
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
                            app.exit(0);
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
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                })
                .build(app)?;

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

                // Start RPC server
                let rpc = RpcServer::new();
                if let Err(e) = rpc.start(manager_clone).await {
                    eprintln!("Warning: RPC server failed to start: {}", e);
                }
            });

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Keep SuperIDM running in background system tray when window is closed (IDM behavior)
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("Error running SuperIDM");
}
