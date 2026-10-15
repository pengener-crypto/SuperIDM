use std::path::{Path, PathBuf};
use std::process::Command;

/// Static embedded extension assets
const EXT_MANIFEST: &str = include_str!("../../../extension/manifest.json");
const EXT_BACKGROUND: &str = include_str!("../../../extension/background.js");
const EXT_CONTENT: &str = include_str!("../../../extension/content.js");
const EXT_OVERLAY_CSS: &str = include_str!("../../../extension/overlay.css");
const EXT_POPUP_HTML: &str = include_str!("../../../extension/popup.html");
const EXT_POPUP_CSS: &str = include_str!("../../../extension/popup.css");
const EXT_POPUP_JS: &str = include_str!("../../../extension/popup.js");
const EXT_INSTAGRAM_HELPER: &str = include_str!("../../../extension/instagram_helper.js");
const EXT_GUIDE: &str = include_str!("../../../extension/طريقة_التثبيت_اليدوي.txt");

const ICON_16: &[u8] = include_bytes!("../../../extension/icons/icon16.png");
const ICON_32: &[u8] = include_bytes!("../../../extension/icons/icon32.png");
const ICON_48: &[u8] = include_bytes!("../../../extension/icons/icon48.png");
const ICON_128: &[u8] = include_bytes!("../../../extension/icons/icon128.png");

/// Get the persistent directory where the embedded extension resides.
pub fn get_embedded_extension_dir() -> PathBuf {
    if let Some(app_data) = dirs::data_dir() {
        app_data.join("SuperIDM").join("extension")
    } else {
        std::env::temp_dir().join("SuperIDM").join("extension")
    }
}

/// Extract and sync embedded extension files into AppData on app startup.
pub fn extract_and_sync_extension(rpc_token: &str) -> Result<PathBuf, String> {
    let ext_dir = get_embedded_extension_dir();
    let icons_dir = ext_dir.join("icons");

    std::fs::create_dir_all(&icons_dir).map_err(|e| e.to_string())?;

    // Write text files
    std::fs::write(ext_dir.join("manifest.json"), EXT_MANIFEST).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("background.js"), EXT_BACKGROUND).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("content.js"), EXT_CONTENT).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("overlay.css"), EXT_OVERLAY_CSS).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("popup.html"), EXT_POPUP_HTML).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("popup.css"), EXT_POPUP_CSS).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("popup.js"), EXT_POPUP_JS).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("instagram_helper.js"), EXT_INSTAGRAM_HELPER).map_err(|e| e.to_string())?;
    std::fs::write(ext_dir.join("طريقة_التثبيت_اليدوي.txt"), EXT_GUIDE).map_err(|e| e.to_string())?;

    // Write icons
    std::fs::write(icons_dir.join("icon16.png"), ICON_16).map_err(|e| e.to_string())?;
    std::fs::write(icons_dir.join("icon32.png"), ICON_32).map_err(|e| e.to_string())?;
    std::fs::write(icons_dir.join("icon48.png"), ICON_48).map_err(|e| e.to_string())?;
    std::fs::write(icons_dir.join("icon128.png"), ICON_128).map_err(|e| e.to_string())?;

    // Write synced RPC token
    let token_json = serde_json::json!({ "token": rpc_token });
    let _ = std::fs::write(ext_dir.join("rpc_token.json"), token_json.to_string());

    // Also sync to Desktop folder if present
    if let Some(desktop) = dirs::desktop_dir() {
        let desk_ext = desktop.join("SuperIDM-Extension");
        let desk_icons = desk_ext.join("icons");
        if let Ok(_) = std::fs::create_dir_all(&desk_icons) {
            let _ = std::fs::write(desk_ext.join("manifest.json"), EXT_MANIFEST);
            let _ = std::fs::write(desk_ext.join("background.js"), EXT_BACKGROUND);
            let _ = std::fs::write(desk_ext.join("content.js"), EXT_CONTENT);
            let _ = std::fs::write(desk_ext.join("overlay.css"), EXT_OVERLAY_CSS);
            let _ = std::fs::write(desk_ext.join("popup.html"), EXT_POPUP_HTML);
            let _ = std::fs::write(desk_ext.join("popup.css"), EXT_POPUP_CSS);
            let _ = std::fs::write(desk_ext.join("popup.js"), EXT_POPUP_JS);
            let _ = std::fs::write(desk_ext.join("instagram_helper.js"), EXT_INSTAGRAM_HELPER);
            let _ = std::fs::write(desk_ext.join("طريقة_التثبيت_اليدوي.txt"), EXT_GUIDE);
            let _ = std::fs::write(desk_icons.join("icon16.png"), ICON_16);
            let _ = std::fs::write(desk_icons.join("icon32.png"), ICON_32);
            let _ = std::fs::write(desk_icons.join("icon48.png"), ICON_48);
            let _ = std::fs::write(desk_icons.join("icon128.png"), ICON_128);
            let _ = std::fs::write(desk_ext.join("rpc_token.json"), token_json.to_string());
        }
    }

    Ok(ext_dir)
}

/// Register Windows Registry Keys for Chromium External Extensions.
pub fn register_browser_registry(browser: &str) -> Result<String, String> {
    let ext_dir = get_embedded_extension_dir();
    let ext_path_str = ext_dir.to_string_lossy().to_string();

    // Constant extension ID or external registry key
    let reg_keys = match browser.to_lowercase().as_str() {
        "chrome" | "google chrome" => vec![
            r"HKCU\Software\Google\Chrome\Extensions\superidm_download_accelerator",
        ],
        "edge" | "microsoft edge" => vec![
            r"HKCU\Software\Microsoft\Edge\Extensions\superidm_download_accelerator",
        ],
        "brave" => vec![
            r"HKCU\Software\BraveSoftware\Brave-Browser\Extensions\superidm_download_accelerator",
        ],
        "opera" => vec![
            r"HKCU\Software\Opera Software\Extensions\superidm_download_accelerator",
        ],
        "vivaldi" => vec![
            r"HKCU\Software\Vivaldi\Extensions\superidm_download_accelerator",
        ],
        _ => vec![
            r"HKCU\Software\Google\Chrome\Extensions\superidm_download_accelerator",
            r"HKCU\Software\Microsoft\Edge\Extensions\superidm_download_accelerator",
            r"HKCU\Software\BraveSoftware\Brave-Browser\Extensions\superidm_download_accelerator",
            r"HKCU\Software\Opera Software\Extensions\superidm_download_accelerator",
            r"HKCU\Software\Vivaldi\Extensions\superidm_download_accelerator",
        ],
    };

    for key in reg_keys {
        // reg add <key> /v path /t REG_SZ /d "<path>" /f
        let _ = Command::new("reg")
            .args(&["add", key, "/v", "path", "/t", "REG_SZ", "/d", &ext_path_str, "/f"])
            .output();

        let _ = Command::new("reg")
            .args(&["add", key, "/v", "version", "/t", "REG_SZ", "/d", "1.0.0", "/f"])
            .output();
    }

    Ok(format!("Extension registered for {}", browser))
}

/// Open browser to extensions management page or official store with 1-click loading assistance
pub fn launch_browser_extension_page(browser: &str) -> Result<(), String> {
    let edge_store_url = "https://microsoftedge.microsoft.com/addons/detail/superidm-%E2%80%94-download-accel/kajmocggiabnnpepieigndpjefjbnjem";
    let is_edge_store = browser.eq_ignore_ascii_case("edge_store");
    let is_edge = matches!(browser.to_lowercase().as_str(), "edge" | "microsoft edge" | "edge_store" | "edge_dev");
    
    let target_url = match browser.to_lowercase().as_str() {
        "edge_store" => edge_store_url,
        "edge" | "microsoft edge" | "edge_dev" => "edge://extensions",
        "brave" => "brave://extensions",
        "opera" => "opera://extensions",
        "vivaldi" => "vivaldi://extensions",
        _ => "chrome://extensions",
    };

    #[cfg(target_os = "windows")]
    {
        let browser_candidates: Vec<PathBuf> = match browser.to_lowercase().as_str() {
            "edge" | "microsoft edge" | "edge_store" | "edge_dev" => vec![
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
                PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            ],
            "brave" => vec![
                PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"),
                dirs::data_local_dir().map(|p| p.join(r"BraveSoftware\Brave-Browser\Application\brave.exe")).unwrap_or_default(),
            ],
            "opera" => vec![
                dirs::data_local_dir().map(|p| p.join(r"Programs\Opera\launcher.exe")).unwrap_or_default(),
                PathBuf::from(r"C:\Program Files\Opera\launcher.exe"),
            ],
            _ => vec![
                PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
                dirs::data_local_dir().map(|p| p.join(r"Google\Chrome\Application\chrome.exe")).unwrap_or_default(),
            ],
        };

        // Determine extension directory to pass --load-extension for non-store browsers
        let ext_dir = if let Some(desk) = dirs::desktop_dir() {
            let d = desk.join("SuperIDM-Extension");
            if d.exists() { d } else { get_embedded_extension_dir() }
        } else {
            get_embedded_extension_dir()
        };
        let load_ext_arg = format!("--load-extension={}", ext_dir.to_string_lossy());

        let mut launched = false;
        for exe in browser_candidates {
            if exe.exists() {
                let mut cmd = Command::new(&exe);
                if is_edge_store {
                    cmd.arg(target_url);
                } else {
                    cmd.args(&[&load_ext_arg, target_url]);
                }
                if cmd.spawn().is_ok() {
                    launched = true;
                    break;
                }
            }
        }

        // If not found by candidate paths, launch via cmd start with browser command name
        if !launched {
            let browser_name = match browser.to_lowercase().as_str() {
                "edge" | "microsoft edge" => "msedge",
                "brave" => "brave",
                "opera" => "opera",
                _ => "chrome",
            };
            if is_edge {
                let _ = Command::new("cmd")
                    .args(&["/c", "start", "", target_url])
                    .spawn();
            } else {
                let _ = Command::new("cmd")
                    .args(&["/c", "start", "", browser_name, &load_ext_arg, target_url])
                    .spawn();
            }
        }
    }

    Ok(())
}
