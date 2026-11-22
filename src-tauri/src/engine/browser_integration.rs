use std::path::PathBuf;
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

    // NOTE: We no longer create a `SuperIDM-Extension` folder on the user's Desktop.
    // The extension is installed from the Chrome Web Store / Edge Add-ons, and the
    // manual (unpacked) copy lives only in %APPDATA%\SuperIDM\extension.

    Ok(ext_dir)
}

/// Register Windows Registry External Extension keys using official Store IDs + update_url
/// (Chromium on Windows ignores unpacked `path` keys and non-32-char extension IDs).
pub fn register_browser_registry(browser: &str) -> Result<String, String> {
    let b = browser.to_lowercase();
    if b == "firefox" || b == "mozilla firefox" {
        return Ok("Firefox uses official AMO Add-ons Store installation".to_string());
    }

    const CHROME_EXT_ID: &str = "hacedobdgfgjhfoldlhiaddmlndkofdo";
    const CHROME_UPDATE_URL: &str = "https://clients2.google.com/service/update2/crx";
    const EDGE_EXT_ID: &str = "kajmocggiabnnpepieigndpjefjbnjem";
    const EDGE_UPDATE_URL: &str = "https://edge.microsoft.com/extensionwebstorebase/v1/crx";

    let entries: Vec<(String, &str)> = match b.as_str() {
        "chrome" | "google chrome" => vec![(
            format!(r"HKCU\Software\Google\Chrome\Extensions\{}", CHROME_EXT_ID),
            CHROME_UPDATE_URL,
        )],
        "edge" | "microsoft edge" => vec![(
            format!(r"HKCU\Software\Microsoft\Edge\Extensions\{}", EDGE_EXT_ID),
            EDGE_UPDATE_URL,
        )],
        "brave" => vec![(
            format!(r"HKCU\Software\BraveSoftware\Brave-Browser\Extensions\{}", CHROME_EXT_ID),
            CHROME_UPDATE_URL,
        )],
        "opera" => vec![(
            format!(r"HKCU\Software\Opera Software\Extensions\{}", CHROME_EXT_ID),
            CHROME_UPDATE_URL,
        )],
        "vivaldi" => vec![(
            format!(r"HKCU\Software\Vivaldi\Extensions\{}", CHROME_EXT_ID),
            CHROME_UPDATE_URL,
        )],
        _ => vec![
            (format!(r"HKCU\Software\Google\Chrome\Extensions\{}", CHROME_EXT_ID), CHROME_UPDATE_URL),
            (format!(r"HKCU\Software\Microsoft\Edge\Extensions\{}", EDGE_EXT_ID), EDGE_UPDATE_URL),
            (format!(r"HKCU\Software\BraveSoftware\Brave-Browser\Extensions\{}", CHROME_EXT_ID), CHROME_UPDATE_URL),
            (format!(r"HKCU\Software\Opera Software\Extensions\{}", CHROME_EXT_ID), CHROME_UPDATE_URL),
            (format!(r"HKCU\Software\Vivaldi\Extensions\{}", CHROME_EXT_ID), CHROME_UPDATE_URL),
        ],
    };

    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        for (key, update_url) in &entries {
            let _ = Command::new("reg")
                .args(["add", key, "/v", "update_url", "/t", "REG_SZ", "/d", update_url, "/f"])
                .creation_flags(0x08000000)
                .output();
            let _ = Command::new("reg")
                .args(["add", key, "/v", "version", "/t", "REG_SZ", "/d", env!("CARGO_PKG_VERSION"), "/f"])
                .creation_flags(0x08000000)
                .output();
        }
    }

    Ok(format!("Extension registered for {}", browser))
}

/// Open browser to extensions management page or official store with 1-click loading assistance
pub fn launch_browser_extension_page(browser: &str) -> Result<(), String> {
    let edge_store_url = "https://microsoftedge.microsoft.com/addons/detail/superidm-%E2%80%94-download-accel/kajmocggiabnnpepieigndpjefjbnjem";
    let chrome_store_url = "https://chromewebstore.google.com/detail/superidm-%E2%80%94-download-accel/hacedobdgfgjhfoldlhiaddmlndkofdo?authuser=0&hl=en";
    let firefox_store_url = "https://addons.mozilla.org/en-US/firefox/addon/superidm-download-accelerator/";

    let b = browser.to_lowercase();
    let target_url = match b.as_str() {
        "edge_store" => edge_store_url,
        "chrome_store" => chrome_store_url,
        "firefox_store" | "firefox" | "mozilla firefox" => firefox_store_url,
        "edge" | "microsoft edge" => edge_store_url,
        "chrome" | "google chrome" | "brave" | "opera" | "vivaldi" => chrome_store_url,
        "edge_dev" => "edge://extensions",
        "chrome_dev" => "chrome://extensions",
        "brave_dev" => "brave://extensions",
        "opera_dev" => "opera://extensions",
        "vivaldi_dev" => "vivaldi://extensions",
        _ => chrome_store_url,
    };

    #[cfg(target_os = "windows")]
    {
        let browser_candidates: Vec<PathBuf> = match b.as_str() {
            "edge" | "microsoft edge" | "edge_store" | "edge_dev" => vec![
                PathBuf::from(r"C:\Program Files (x86)\Microsoft\Edge\Application\msedge.exe"),
                PathBuf::from(r"C:\Program Files\Microsoft\Edge\Application\msedge.exe"),
            ],
            "firefox" | "mozilla firefox" | "firefox_store" => vec![
                PathBuf::from(r"C:\Program Files\Mozilla Firefox\firefox.exe"),
                PathBuf::from(r"C:\Program Files (x86)\Mozilla Firefox\firefox.exe"),
                dirs::data_local_dir().map(|p| p.join(r"Mozilla Firefox\firefox.exe")).unwrap_or_default(),
            ],
            "brave" | "brave_dev" => vec![
                PathBuf::from(r"C:\Program Files\BraveSoftware\Brave-Browser\Application\brave.exe"),
                PathBuf::from(r"C:\Program Files (x86)\BraveSoftware\Brave-Browser\Application\brave.exe"),
                dirs::data_local_dir().map(|p| p.join(r"BraveSoftware\Brave-Browser\Application\brave.exe")).unwrap_or_default(),
            ],
            "opera" | "opera_dev" => vec![
                dirs::data_local_dir().map(|p| p.join(r"Programs\Opera\launcher.exe")).unwrap_or_default(),
                PathBuf::from(r"C:\Program Files\Opera\launcher.exe"),
            ],
            "vivaldi" | "vivaldi_dev" => vec![
                dirs::data_local_dir().map(|p| p.join(r"Vivaldi\Application\vivaldi.exe")).unwrap_or_default(),
                PathBuf::from(r"C:\Program Files\Vivaldi\Application\vivaldi.exe"),
            ],
            _ => vec![
                PathBuf::from(r"C:\Program Files\Google\Chrome\Application\chrome.exe"),
                PathBuf::from(r"C:\Program Files (x86)\Google\Chrome\Application\chrome.exe"),
                dirs::data_local_dir().map(|p| p.join(r"Google\Chrome\Application\chrome.exe")).unwrap_or_default(),
            ],
        };

        let ext_dir = get_embedded_extension_dir();
        let load_ext_arg = format!("--load-extension={}", ext_dir.to_string_lossy());

        let mut launched = false;
        let is_http_target = target_url.starts_with("http://") || target_url.starts_with("https://");

        for exe in browser_candidates {
            if exe.exists() {
                let mut cmd = Command::new(&exe);
                if is_http_target {
                    cmd.arg(target_url);
                } else {
                    cmd.args([&load_ext_arg, target_url]);
                }
                if cmd.spawn().is_ok() {
                    launched = true;
                    break;
                }
            }
        }

        // Safe fallback: explorer.exe delegates to ShellExecute without cmd.exe `&` parsing or console flash
        if !launched {
            if is_http_target {
                let _ = Command::new("explorer.exe").arg(target_url).spawn();
            } else {
                let browser_exe = match b.as_str() {
                    "edge" | "microsoft edge" | "edge_dev" => "msedge.exe",
                    "brave" | "brave_dev" => "brave.exe",
                    "opera" | "opera_dev" => "opera.exe",
                    "vivaldi" | "vivaldi_dev" => "vivaldi.exe",
                    _ => "chrome.exe",
                };
                if Command::new(browser_exe).args([&load_ext_arg, target_url]).spawn().is_err() {
                    let _ = Command::new("explorer.exe").arg(ext_dir).spawn();
                }
            }
        }
    }

    Ok(())
}
