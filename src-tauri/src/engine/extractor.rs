use std::path::PathBuf;
use std::process::Command;
use reqwest::Client;
use serde_json::Value;

use super::session::DEFAULT_USER_AGENT;
use super::types::{MediaExtractionInfo, MediaFormat, PlaylistItem, PlaylistExtractionInfo};

/// Universal Media Extractor Pipeline.
pub struct MediaExtractor;

/// In-memory ring buffer of recent engine events/errors, included in diagnostic reports.
fn engine_log_buffer() -> &'static parking_lot::Mutex<std::collections::VecDeque<String>> {
    static BUF: std::sync::OnceLock<parking_lot::Mutex<std::collections::VecDeque<String>>> =
        std::sync::OnceLock::new();
    BUF.get_or_init(|| parking_lot::Mutex::new(std::collections::VecDeque::with_capacity(200)))
}

/// Record a line in the engine diagnostic log (also printed to stderr).
pub fn record_engine_log(msg: impl AsRef<str>) {
    let line = format!(
        "[{}] {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S"),
        msg.as_ref()
    );
    eprintln!("{}", line);
    let mut buf = engine_log_buffer().lock();
    if buf.len() >= 200 {
        buf.pop_front();
    }
    buf.push_back(line);
}

/// Snapshot of recent engine log lines (oldest first).
pub fn recent_engine_log() -> Vec<String> {
    engine_log_buffer().lock().iter().cloned().collect()
}

/// Real, measured state of the media engine components (no assumed values).
#[derive(Debug, Clone, serde::Serialize)]
pub struct EngineStatus {
    pub ytdlp_path: Option<String>,
    pub ytdlp_version: Option<String>,
    pub ffmpeg_path: Option<String>,
    pub ffmpeg_ok: bool,
    pub js_runtime: Option<String>,
    pub js_ok: bool,
    pub ready: bool,
}

fn run_quiet(exe: &std::path::Path, args: &[&str]) -> Option<std::process::Output> {
    let mut cmd = Command::new(exe);
    cmd.args(args);
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
    }
    cmd.output().ok()
}

impl MediaExtractor {
    /// Probe every media engine component by actually executing it.
    /// Blocking: call from `spawn_blocking`.
    pub fn engine_status() -> EngineStatus {
        let ytdlp = Self::find_ytdlp_binary();
        let ytdlp_version = ytdlp.as_ref().and_then(|p| Self::verify_ytdlp_binary(p));

        let ffmpeg = Self::find_ffmpeg_binary();
        let ffmpeg_ok = ffmpeg
            .as_ref()
            .and_then(|p| run_quiet(p, &["-hide_banner", "-version"]))
            .map(|o| o.status.success())
            .unwrap_or(false);

        let js_runtime = Self::js_runtime_arg();
        let js_ok = js_runtime
            .as_ref()
            .and_then(|rt| {
                let (kind, path) = rt.split_once(':')?;
                let exe = std::path::PathBuf::from(path);
                let out = match kind {
                    "quickjs" | "node" => run_quiet(&exe, &["-e", "console.log(6*7)"]),
                    "deno" => run_quiet(&exe, &["eval", "console.log(6*7)"]),
                    _ => None,
                }?;
                Some(out.status.success() && String::from_utf8_lossy(&out.stdout).contains("42"))
            })
            .unwrap_or(false);

        let ready = ytdlp_version.is_some() && ffmpeg_ok && js_ok;
        EngineStatus {
            ytdlp_path: ytdlp.map(|p| p.to_string_lossy().to_string()),
            ytdlp_version,
            ffmpeg_path: ffmpeg.map(|p| p.to_string_lossy().to_string()),
            ffmpeg_ok,
            js_runtime,
            js_ok,
            ready,
        }
    }

    /// Check if a URL belongs to a media stream or streaming platform
    pub fn is_media_url(url: &str) -> bool {
        let u = url.to_lowercase();
        u.contains("youtube.com") || u.contains("youtu.be") ||
        u.contains("cinemana.shabakaty.com") ||
        u.contains("tiktok.com") || u.contains("vimeo.com") ||
        u.contains("dailymotion.com") || u.contains("facebook.com") ||
        u.contains("fb.com") || u.contains("instagram.com") ||
        u.contains("twitter.com") || u.contains("x.com") ||
        u.contains(".m3u8") || u.contains(".mpd")
    }

    /// Global async mutex serializing all component bootstrap and update operations
    /// (yt-dlp, qjs/deno, ffmpeg) so concurrent tasks never clobber temp files or download
    /// the same binary simultaneously.
    fn component_lock() -> &'static tokio::sync::Mutex<()> {
        static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
        LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    /// Locate a bundled helper executable (`yt-dlp.exe`, `qjs.exe`, `ffmpeg.exe`) across
    /// MSIX, NSIS, Portable, and development directory layouts.
    fn find_bundled_file(filename: &str, min_size: u64) -> Option<PathBuf> {
        let exe = std::env::current_exe().ok()?;
        let app_dir = exe.parent()?;
        let mut candidates = vec![
            app_dir.join(filename),
            app_dir.join("bin").join(filename),
            app_dir.join("resources").join(filename),
            app_dir.join("_up_").join("msix_build").join(filename),
        ];
        if let Some(parent) = app_dir.parent() {
            candidates.push(parent.join("msix_build").join(filename));
            if let Some(grand) = parent.parent() {
                candidates.push(grand.join("msix_build").join(filename));
            }
        }
        for p in candidates {
            if let Ok(meta) = std::fs::metadata(&p) {
                if meta.len() >= min_size {
                    return Some(p);
                }
            }
        }
        None
    }

    /// Verify that a candidate `yt-dlp.exe` is a genuine, non-corrupted standalone binary
    /// of version `>= 2026.10.01` (supports `--js-runtimes` and current YouTube challenges).
    /// Cached in memory by `(path, size, mtime)` to avoid repeated process spawns.
    pub fn verify_ytdlp_binary(path: &std::path::Path) -> Option<String> {
        type CacheMap = std::collections::HashMap<
            PathBuf,
            (u64, Option<std::time::SystemTime>, Option<String>),
        >;
        static CACHE: std::sync::OnceLock<parking_lot::Mutex<CacheMap>> =
            std::sync::OnceLock::new();

        let meta = std::fs::metadata(path).ok()?;
        let len = meta.len();
        // Standalone PyInstaller yt-dlp.exe is ~22 MB; reject tiny pip shims or truncated downloads
        if len < 15_000_000 {
            return None;
        }
        let mtime = meta.modified().ok();
        let pb = path.to_path_buf();

        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(std::collections::HashMap::new()));
        {
            let guard = cache.lock();
            if let Some((c_len, c_mtime, c_ver)) = guard.get(&pb) {
                if *c_len == len && *c_mtime == mtime {
                    return c_ver.clone();
                }
            }
        }

        let mut cmd = Command::new(path);
        cmd.arg("--version");
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }

        let ver_opt = cmd
            .output()
            .ok()
            .filter(|o| o.status.success())
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|v| v.len() >= 10 && v.as_str() >= "2026.10.01");

        cache.lock().insert(pb, (len, mtime, ver_opt.clone()));
        ver_opt
    }

    /// Return path to AppData/Local/SuperIDM/bin/yt-dlp.exe
    pub fn get_local_ytdlp_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("SuperIDM").join("bin").join("yt-dlp.exe"))
    }

    /// Official sources for the standalone yt-dlp executable (stable first, nightly as fallback).
    const YTDLP_URLS: [&'static str; 2] = [
        "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp.exe",
        "https://github.com/yt-dlp/yt-dlp-nightly-builds/releases/latest/download/yt-dlp.exe",
    ];

    /// Query GitHub API for latest yt-dlp release tag and ordered download URLs.
    async fn resolve_ytdlp_release(client: &Client) -> (Option<String>, Vec<String>) {
        let mut urls = Vec::new();
        let mut latest_tag: Option<String> = None;
        let api_req = client
            .get("https://api.github.com/repos/yt-dlp/yt-dlp/releases/latest")
            .header("User-Agent", format!("SuperIDM-Core/{}", env!("CARGO_PKG_VERSION")))
            .header("Accept", "application/vnd.github+json")
            .send();
        if let Ok(Ok(resp)) = tokio::time::timeout(std::time::Duration::from_secs(8), api_req).await {
            if resp.status().is_success() {
                if let Ok(json) = resp.json::<Value>().await {
                    if let Some(tag) = json.get("tag_name").and_then(|t| t.as_str()) {
                        latest_tag = Some(tag.trim().to_string());
                    }
                    if let Some(assets) = json.get("assets").and_then(|v| v.as_array()) {
                        for asset in assets {
                            if asset.get("name").and_then(|n| n.as_str()) == Some("yt-dlp.exe") {
                                if let Some(dl) = asset.get("browser_download_url").and_then(|u| u.as_str()) {
                                    urls.push(dl.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }
        for fallback in Self::YTDLP_URLS {
            urls.push(fallback.to_string());
        }
        (latest_tag, urls)
    }

    /// Robust component downloader used for yt-dlp / FFmpeg / QuickJS / Deno bootstrap.
    ///
    /// Iterates across all mirror URLs first before sleeping and retrying, so if GitHub returns
    /// 502/504 in the user's region, it switches to the alternate mirror immediately.
    pub async fn fetch_component(
        client: &Client,
        urls: &[String],
        dest: &std::path::Path,
        min_size: u64,
    ) -> Result<(), String> {
        use futures_util::StreamExt;
        use std::time::Duration;
        use tokio::io::AsyncWriteExt;

        let mut last_err = "no download source available".to_string();
        for round in 1..=2u64 {
            if round > 1 {
                tokio::time::sleep(Duration::from_secs(3)).await;
            }
            for url in urls {
                let host = url.split('/').nth(2).unwrap_or(url).to_string();
                let send = client
                    .get(url)
                    .header("User-Agent", format!("SuperIDM-Core/{}", env!("CARGO_PKG_VERSION")))
                    .send();
                let resp = match tokio::time::timeout(Duration::from_secs(25), send).await {
                    Ok(Ok(r)) if r.status().is_success() => r,
                    Ok(Ok(r)) => {
                        last_err = format!("{} returned HTTP {}", host, r.status());
                        continue;
                    }
                    Ok(Err(e)) => {
                        last_err = format!("cannot reach {} ({})", host, e);
                        continue;
                    }
                    Err(_) => {
                        last_err = format!("{} did not respond within 25s", host);
                        continue;
                    }
                };

                let mut file = match tokio::fs::File::create(dest).await {
                    Ok(f) => f,
                    Err(e) => return Err(format!("cannot write {:?}: {}", dest, e)),
                };
                let mut stream = resp.bytes_stream();
                let mut written = 0u64;
                let mut ok = true;
                loop {
                    match tokio::time::timeout(Duration::from_secs(45), stream.next()).await {
                        Ok(Some(Ok(chunk))) => {
                            if let Err(e) = file.write_all(&chunk).await {
                                last_err = format!("disk write failed: {}", e);
                                ok = false;
                                break;
                            }
                            written += chunk.len() as u64;
                        }
                        Ok(Some(Err(e))) => {
                            last_err = format!("transfer from {} interrupted ({})", host, e);
                            ok = false;
                            break;
                        }
                        Ok(None) => break,
                        Err(_) => {
                            last_err = format!("transfer from {} stalled (no data for 45s)", host);
                            ok = false;
                            break;
                        }
                    }
                }
                let _ = file.flush().await;
                drop(file);

                if ok && written >= min_size {
                    return Ok(());
                }
                if ok {
                    last_err = format!("{} sent a truncated file ({} bytes)", host, written);
                }
                let _ = tokio::fs::remove_file(dest).await;
            }
        }
        Err(last_err)
    }

    /// Locate or auto-download standalone yt-dlp binary so that any clean Windows machine works out-of-the-box.
    pub async fn ensure_ytdlp_binary(client: &Client) -> Result<PathBuf, String> {
        if let Some(p) = Self::find_ytdlp_binary() {
            return Ok(p);
        }

        let _guard = Self::component_lock().lock().await;
        if let Some(p) = Self::find_ytdlp_binary() {
            return Ok(p);
        }

        if let Some(local_bin) = Self::get_local_ytdlp_path() {
            if let Some(parent) = local_bin.parent() {
                let _ = tokio::fs::create_dir_all(parent).await;
            }

            eprintln!("[SuperIDM] Downloading standalone media extractor core to: {:?}", local_bin);
            let tmp_bin = local_bin.with_extension(format!("exe.{}.tmp", uuid::Uuid::new_v4()));
            let (_, urls) = Self::resolve_ytdlp_release(client).await;
            Self::fetch_component(client, &urls, &tmp_bin, 15_000_000)
                .await
                .map_err(|e| format!("Failed to download video extractor core: {}", e))?;
            let _ = tokio::fs::remove_file(&local_bin).await;
            tokio::fs::rename(&tmp_bin, &local_bin)
                .await
                .map_err(|e| format!("Failed activating extractor: {}", e))?;

            return Ok(local_bin);
        }

        Err("Unable to locate or bootstrap media extractor".to_string())
    }

    /// Force-update or verify the latest standalone yt-dlp binary in `%LOCALAPPDATA%/SuperIDM/bin/yt-dlp.exe`.
    ///
    /// Fast-paths in < 1 second when the installed/bundled binary already matches the latest
    /// GitHub release tag, avoiding unnecessary 22 MB re-downloads or `yt-dlp -U` hangs.
    pub async fn update_ytdlp_core(client: &Client) -> Result<String, String> {
        // Ensure JS runtime is present as part of engine health check
        if Self::js_runtime_arg().is_none() {
            let _ = Self::ensure_deno_binary(client).await;
        }

        let _guard = Self::component_lock().lock().await;

        let local_bin = Self::get_local_ytdlp_path()
            .ok_or_else(|| "Local AppData directory not found".to_string())?;

        if let Some(parent) = local_bin.parent() {
            let _ = tokio::fs::create_dir_all(parent).await;
        }

        let current_ver = Self::find_ytdlp_binary().and_then(|p| Self::verify_ytdlp_binary(&p));
        let (latest_tag, urls) = Self::resolve_ytdlp_release(client).await;

        if let (Some(ref cur), Some(ref latest)) = (&current_ver, &latest_tag) {
            if cur >= latest {
                return Ok(format!("Video extractor core is up to date (v{})", cur));
            }
        }

        let tmp_bin = local_bin.with_extension(format!("exe.{}.tmp", uuid::Uuid::new_v4()));
        eprintln!("[SuperIDM] Updating standalone extractor core to: {:?}", tmp_bin);

        match Self::fetch_component(client, &urls, &tmp_bin, 15_000_000).await {
            Ok(()) => {
                let _ = tokio::fs::remove_file(&local_bin).await;
                tokio::fs::rename(&tmp_bin, &local_bin)
                    .await
                    .map_err(|e| format!("Failed committing updated extractor binary: {}", e))?;

                let version = Self::verify_ytdlp_binary(&local_bin)
                    .or(latest_tag)
                    .unwrap_or_else(|| "latest".to_string());

                Ok(format!("Video extractor core updated to version {}", version))
            }
            Err(e) => {
                let _ = tokio::fs::remove_file(&tmp_bin).await;
                // If we already have a verified working modern binary (e.g. bundled v2026.10.26)
                // and GitHub is unreachable, report the active version instead of failing.
                if let Some(cur) = current_ver {
                    return Ok(format!("Video extractor core active (v{})", cur));
                }
                Err(format!("Could not update extractor core: {}", e))
            }
        }
    }

    /// Return path to AppData/Local/SuperIDM/bin/ffmpeg.exe
    pub fn get_local_ffmpeg_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("SuperIDM").join("bin").join("ffmpeg.exe"))
    }

    /// Locate or auto-download standalone FFmpeg binary on-demand.
    pub async fn ensure_ffmpeg_binary(client: &Client) -> Result<PathBuf, String> {
        if let Some(p) = Self::find_ffmpeg_binary() {
            return Ok(p);
        }

        let _guard = Self::component_lock().lock().await;
        if let Some(p) = Self::find_ffmpeg_binary() {
            return Ok(p);
        }

        let local_bin = Self::get_local_ffmpeg_path()
            .ok_or_else(|| "Local AppData directory not found".to_string())?;
        let bin_dir = local_bin.parent().ok_or_else(|| "Invalid bin dir".to_string())?.to_path_buf();
        let _ = tokio::fs::create_dir_all(&bin_dir).await;

        eprintln!("[SuperIDM FFmpeg-Bootstrap] Downloading compact FFmpeg static binary (28 MB gz)...");

        let gz_urls = vec![
            "https://github.com/eugeneware/ffmpeg-static/releases/download/b6.1.1/ffmpeg-win32-x64.gz".to_string(),
        ];
        let tmp_gz = bin_dir.join(format!("ffmpeg_{}.gz.tmp", uuid::Uuid::new_v4()));
        if Self::fetch_component(client, &gz_urls, &tmp_gz, 10_000_000).await.is_ok() {
            let gz_c = tmp_gz.clone();
            let local_c = local_bin.clone();
            let unpack_res = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
                let gz_file = std::fs::File::open(&gz_c)
                    .map_err(|e| format!("Failed to open FFmpeg gz: {}", e))?;
                let mut decoder = flate2::read::GzDecoder::new(gz_file);
                let tmp_exe = local_c.with_extension(format!("exe.{}.tmp", uuid::Uuid::new_v4()));
                let mut out_file = std::fs::File::create(&tmp_exe)
                    .map_err(|e| format!("Failed to create ffmpeg.exe: {}", e))?;
                let bytes = std::io::copy(&mut decoder, &mut out_file)
                    .map_err(|e| format!("Failed to decompress ffmpeg.exe: {}", e))?;
                drop(out_file);
                if bytes < 20_000_000 {
                    let _ = std::fs::remove_file(&tmp_exe);
                    return Err("Decompressed ffmpeg.exe is truncated".to_string());
                }
                let _ = std::fs::remove_file(&local_c);
                std::fs::rename(&tmp_exe, &local_c)
                    .map_err(|e| format!("Failed to activate ffmpeg.exe: {}", e))?;
                Ok(local_c)
            })
            .await
            .map_err(|e| format!("FFmpeg decompression thread failed: {}", e));

            let _ = tokio::fs::remove_file(&tmp_gz).await;
            if let Ok(Ok(p)) = unpack_res {
                return Ok(p);
            }
        }
        let _ = tokio::fs::remove_file(&tmp_gz).await;

        // Fallback to Gyan essentials zip if gz mirror failed
        let download_urls = vec![
            "https://www.gyan.dev/ffmpeg/builds/ffmpeg-release-essentials.zip".to_string(),
        ];
        let tmp_zip = bin_dir.join(format!("ffmpeg_{}.zip.tmp", uuid::Uuid::new_v4()));
        if let Err(e) = Self::fetch_component(client, &download_urls, &tmp_zip, 10_000_000).await {
            return Err(format!("Failed downloading FFmpeg: {}", e));
        }

        let bin_dir_clone = bin_dir.clone();
        let tmp_zip_clone = tmp_zip.clone();
        let extract_res = tokio::task::spawn_blocking(move || {
            Self::extract_ffmpeg_from_zip(&tmp_zip_clone, &bin_dir_clone)
        })
        .await
        .map_err(|e| format!("Extraction thread failed: {}", e))??;

        let _ = tokio::fs::remove_file(&tmp_zip).await;
        Ok(extract_res)
    }

    /// Extract `ffmpeg.exe` and `ffprobe.exe` from the zip archive into `target_dir`.
    pub fn extract_ffmpeg_from_zip(zip_path: &std::path::Path, target_dir: &std::path::Path) -> Result<PathBuf, String> {
        let file = std::fs::File::open(zip_path).map_err(|e| format!("Failed to open downloaded archive: {}", e))?;
        let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid zip archive: {}", e))?;

        let mut ffmpeg_target: Option<PathBuf> = None;

        for i in 0..archive.len() {
            let mut entry = match archive.by_index(i) {
                Ok(e) => e,
                Err(_) => continue,
            };

            let name = entry.name().to_string();
            let is_ffmpeg = name.ends_with("ffmpeg.exe");
            let is_ffprobe = name.ends_with("ffprobe.exe");

            if is_ffmpeg || is_ffprobe {
                let fname = if is_ffmpeg { "ffmpeg.exe" } else { "ffprobe.exe" };
                let out_path = target_dir.join(fname);
                let tmp_path = target_dir.join(format!("{}.{}.tmp", fname, uuid::Uuid::new_v4()));

                let mut out_file = std::fs::File::create(&tmp_path)
                    .map_err(|e| format!("Failed to create {}: {}", fname, e))?;

                std::io::copy(&mut entry, &mut out_file)
                    .map_err(|e| format!("Failed to extract {}: {}", fname, e))?;

                drop(out_file);
                let _ = std::fs::remove_file(&out_path);
                let _ = std::fs::rename(&tmp_path, &out_path);

                if is_ffmpeg {
                    ffmpeg_target = Some(out_path);
                }
            }
        }

        ffmpeg_target.ok_or_else(|| "ffmpeg.exe not found in downloaded archive".to_string())
    }

    /// Return path to AppData/Local/SuperIDM/bin/qjs.exe
    pub fn get_local_qjs_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("SuperIDM").join("bin").join("qjs.exe"))
    }

    /// Locate QuickJS-ng (`qjs.exe`) runtime (bundled, self-downloaded, or on PATH).
    pub fn find_qjs_binary() -> Option<PathBuf> {
        if let Some(b) = Self::find_bundled_file("qjs.exe", 500_000) {
            return Some(b);
        }
        if let Some(local) = Self::get_local_qjs_path() {
            if std::fs::metadata(&local).map(|m| m.len() >= 500_000).unwrap_or(false) {
                return Some(local);
            }
        }
        Self::find_executable("qjs")
    }

    /// Return path to AppData/Local/SuperIDM/bin/deno.exe
    pub fn get_local_deno_path() -> Option<PathBuf> {
        dirs::data_local_dir().map(|d| d.join("SuperIDM").join("bin").join("deno.exe"))
    }

    /// Locate a Deno runtime (bundled, self-downloaded, or on PATH).
    pub fn find_deno_binary() -> Option<PathBuf> {
        if let Some(b) = Self::find_bundled_file("deno.exe", 10_000_000) {
            return Some(b);
        }
        if let Some(local) = Self::get_local_deno_path() {
            if std::fs::metadata(&local).map(|m| m.len() > 10_000_000).unwrap_or(false) {
                return Some(local);
            }
        }
        Self::find_executable("deno")
    }

    /// Ensure an external JavaScript runtime is available for solving YouTube signature / "n"
    /// challenges. Prefers QuickJS-ng (`qjs.exe`, only 1.8 MB direct binary) and falls back to Deno.
    pub async fn ensure_deno_binary(client: &Client) -> Result<PathBuf, String> {
        if let Some(q) = Self::find_qjs_binary() {
            return Ok(q);
        }
        if let Some(d) = Self::find_deno_binary() {
            return Ok(d);
        }

        let _guard = Self::component_lock().lock().await;
        if let Some(q) = Self::find_qjs_binary() {
            return Ok(q);
        }
        if let Some(d) = Self::find_deno_binary() {
            return Ok(d);
        }

        let qjs_local = Self::get_local_qjs_path()
            .ok_or_else(|| "Local AppData directory not found".to_string())?;
        let bin_dir = qjs_local.parent().ok_or_else(|| "Invalid bin dir".to_string())?.to_path_buf();
        let _ = tokio::fs::create_dir_all(&bin_dir).await;

        let qjs_urls = vec![
            "https://github.com/quickjs-ng/quickjs/releases/download/v0.18.0/qjs-windows-x86_64.exe".to_string(),
            "https://github.com/quickjs-ng/quickjs/releases/latest/download/qjs-windows-x86_64.exe".to_string(),
        ];
        let tmp_qjs = qjs_local.with_extension(format!("exe.{}.tmp", uuid::Uuid::new_v4()));
        if Self::fetch_component(client, &qjs_urls, &tmp_qjs, 500_000).await.is_ok() {
            let _ = tokio::fs::remove_file(&qjs_local).await;
            if tokio::fs::rename(&tmp_qjs, &qjs_local).await.is_ok() {
                return Ok(qjs_local);
            }
        }
        let _ = tokio::fs::remove_file(&tmp_qjs).await;

        // Fallback to Deno if QuickJS mirror failed
        let local = Self::get_local_deno_path()
            .ok_or_else(|| "Local AppData directory not found".to_string())?;
        let mut urls = Vec::new();
        if let Ok(Ok(r)) = tokio::time::timeout(
            std::time::Duration::from_secs(8),
            client.get("https://dl.deno.land/release-latest.txt").send(),
        ).await {
            if r.status().is_success() {
                if let Ok(tag) = r.text().await {
                    let tag = tag.trim();
                    if tag.starts_with('v') && tag.len() < 24 && !tag.contains('/') {
                        urls.push(format!("https://dl.deno.land/release/{}/deno-x86_64-pc-windows-msvc.zip", tag));
                    }
                }
            }
        }
        urls.push("https://github.com/denoland/deno/releases/latest/download/deno-x86_64-pc-windows-msvc.zip".to_string());

        let tmp_zip = bin_dir.join(format!("deno_{}.zip.tmp", uuid::Uuid::new_v4()));
        Self::fetch_component(client, &urls, &tmp_zip, 10_000_000)
            .await
            .map_err(|e| format!("Failed downloading JavaScript runtime: {}", e))?;

        let zip_c = tmp_zip.clone();
        let local_c = local.clone();
        let res = tokio::task::spawn_blocking(move || -> Result<PathBuf, String> {
            let file = std::fs::File::open(&zip_c).map_err(|e| format!("Failed to open Deno archive: {}", e))?;
            let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Invalid Deno archive: {}", e))?;
            for i in 0..archive.len() {
                let mut entry = match archive.by_index(i) { Ok(e) => e, Err(_) => continue };
                if entry.name().ends_with("deno.exe") {
                    let tmp = local_c.with_extension(format!("exe.{}.tmp", uuid::Uuid::new_v4()));
                    let mut out = std::fs::File::create(&tmp).map_err(|e| format!("Failed to create deno.exe: {}", e))?;
                    std::io::copy(&mut entry, &mut out).map_err(|e| format!("Failed to extract deno.exe: {}", e))?;
                    drop(out);
                    let _ = std::fs::remove_file(&local_c);
                    std::fs::rename(&tmp, &local_c).map_err(|e| format!("Failed to activate deno.exe: {}", e))?;
                    return Ok(local_c.clone());
                }
            }
            Err("deno.exe not found in downloaded archive".to_string())
        }).await.map_err(|e| format!("Deno extraction thread failed: {}", e))?;

        let _ = tokio::fs::remove_file(&tmp_zip).await;
        res
    }

    /// Build the `--js-runtimes` value for yt-dlp, preferring QuickJS-ng (`qjs.exe`), then Deno, then Node.
    pub fn js_runtime_arg() -> Option<String> {
        if let Some(q) = Self::find_qjs_binary() {
            return Some(format!("quickjs:{}", q.to_string_lossy()));
        }
        if let Some(d) = Self::find_deno_binary() {
            return Some(format!("deno:{}", d.to_string_lossy()));
        }
        if let Some(n) = Self::find_executable("node") {
            return Some(format!("node:{}", n.to_string_lossy()));
        }
        None
    }

    /// True for YouTube page / short links (these require the JS runtime).
    pub fn is_youtube_url(url: &str) -> bool {
        let u = url.to_lowercase();
        u.contains("youtube.com") || u.contains("youtu.be") || u.contains("youtube-nocookie.com")
    }

    /// Check if an executable exists on PATH or common tools
    pub fn find_executable(name: &str) -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new("where");
            let target = if name.ends_with(".exe") { name.to_string() } else { format!("{}.exe", name) };
            cmd.arg(&target);
            cmd.creation_flags(0x08000000);
            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = out_str.lines().next() {
                        let pb = PathBuf::from(first_line.trim());
                        if pb.exists() {
                            return Some(pb);
                        }
                    }
                }
            }
        }
        #[cfg(not(target_os = "windows"))]
        {
            let mut cmd = Command::new("which");
            cmd.arg(name);
            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    let out_str = String::from_utf8_lossy(&output.stdout);
                    if let Some(first_line) = out_str.lines().next() {
                        let pb = PathBuf::from(first_line.trim());
                        if pb.exists() {
                            return Some(pb);
                        }
                    }
                }
            }
        }
        None
    }

    /// Locate a verified modern yt-dlp binary across App Dir, SuperIDM bin, and PATH.
    /// Automatically purges corrupted or outdated copies in `%LOCALAPPDATA%/SuperIDM/bin/yt-dlp.exe`.
    pub fn find_ytdlp_binary() -> Option<PathBuf> {
        let bundled = Self::find_bundled_file("yt-dlp.exe", 15_000_000);

        if let Some(local_bin) = Self::get_local_ytdlp_path() {
            if local_bin.exists() {
                if let Some(local_ver) = Self::verify_ytdlp_binary(&local_bin) {
                    if let Some(ref b_path) = bundled {
                        if let Some(b_ver) = Self::verify_ytdlp_binary(b_path) {
                            if b_ver > local_ver {
                                let _ = std::fs::copy(b_path, &local_bin);
                                return Some(b_path.clone());
                            }
                        }
                    }
                    return Some(local_bin);
                } else {
                    // Corrupted, truncated, or outdated (< 2026.10.01) local binary — delete it!
                    record_engine_log(format!("[engine] purged invalid/outdated local yt-dlp: {:?}", local_bin));
                    let _ = std::fs::remove_file(&local_bin);
                }
            }
        }

        if let Some(b_path) = bundled {
            if Self::verify_ytdlp_binary(&b_path).is_some() {
                return Some(b_path);
            }
        }

        if let Some(p) = Self::find_executable("yt-dlp") {
            if Self::verify_ytdlp_binary(&p).is_some() {
                return Some(p);
            }
        }

        if let Some(home) = dirs::home_dir() {
            let candidates = vec![
                home.join(r"AppData\Local\Microsoft\WinGet\Links\yt-dlp.exe"),
                home.join(r"AppData\Local\bin\yt-dlp.exe"),
                home.join(r".local\bin\yt-dlp.exe"),
            ];
            for path in candidates {
                if path.exists() && Self::verify_ytdlp_binary(&path).is_some() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Locate ffmpeg binary on the system across App Dir, SuperIDM bin, and system candidates.
    pub fn find_ffmpeg_binary() -> Option<PathBuf> {
        if let Some(b) = Self::find_bundled_file("ffmpeg.exe", 10_000_000) {
            return Some(b);
        }

        if let Some(local_bin) = Self::get_local_ffmpeg_path() {
            if local_bin.exists() {
                if std::fs::metadata(&local_bin).map(|m| m.len() >= 10_000_000).unwrap_or(false) {
                    return Some(local_bin);
                } else {
                    let _ = std::fs::remove_file(&local_bin);
                }
            }
        }

        if let Some(p) = Self::find_executable("ffmpeg") {
            return Some(p);
        }

        if let Some(home) = dirs::home_dir() {
            let candidates = vec![
                home.join(r"AppData\Local\Microsoft\WinGet\Links\ffmpeg.exe"),
                home.join(r"AppData\Local\Microsoft\WinGet\Packages\Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe\ffmpeg-9.0-full_build\bin\ffmpeg.exe"),
                home.join(r"AppData\Local\GhostDownloader\FFmpeg\ffmpeg.exe"),
                home.join(r"AppData\Local\Programs\Stremio\ffmpeg.exe"),
                home.join(r"AppData\Local\bin\ffmpeg.exe"),
                PathBuf::from(r"C:\ProgramData\chocolatey\bin\ffmpeg.exe"),
                PathBuf::from(r"C:\ffmpeg\bin\ffmpeg.exe"),
                PathBuf::from(r"C:\Program Files\ffmpeg\bin\ffmpeg.exe"),
            ];
            for path in candidates {
                if path.exists() {
                    return Some(path);
                }
            }
        }

        None
    }

    /// Extract a clean HTTP/HTTPS URL from markdown links e.g. [Title](https://...) or raw text.
    pub fn clean_target_url(raw: &str) -> String {
        let trimmed = raw.trim();
        // 1. Markdown link format: [title](url)
        if let Some(start) = trimmed.find("](") {
            if let Some(end) = trimmed[start + 2..].find(')') {
                let inner = &trimmed[start + 2..start + 2 + end];
                if inner.starts_with("http://") || inner.starts_with("https://") {
                    return inner.trim().to_string();
                }
            }
        }
        // 2. Text containing URL
        if let Some(pos) = trimmed.find("https://").or_else(|| trimmed.find("http://")) {
            let after = &trimmed[pos..];
            let mut end_pos = after
                .find(|c: char| c.is_whitespace() || c == ')' || c == ']' || c == '"' || c == '\'')
                .unwrap_or(after.len());
            // Guard against accidental double-paste (e.g. "https://...start_radio=1https://...")
            // where the second scheme is not an embedded `=https://` query parameter value.
            let scheme_skip = if after.starts_with("https://") { 8 } else { 7 };
            if end_pos > scheme_skip {
                let tail = &after[scheme_skip..end_pos];
                for marker in ["https://", "http://"] {
                    if let Some(dup_rel) = tail.find(marker) {
                        let dup_idx = scheme_skip + dup_rel;
                        let prev = after[..dup_idx].chars().last().unwrap_or(' ');
                        if prev != '=' {
                            end_pos = end_pos.min(dup_idx);
                        }
                    }
                }
            }
            return after[..end_pos].to_string();
        }
        trimmed.to_string()
    }

    /// Check if a URL is from a known video/media platform that requires signature/token extraction.
    /// Comprehensive list of video/audio platforms supported via yt-dlp.
    /// yt-dlp supports 1000+ sites; this list covers the major ones for UI routing.
    pub fn is_platform_url(url: &str) -> bool {
        let cleaned = Self::clean_target_url(url);
        let u = cleaned.to_lowercase();
        // --- Major Video Platforms ---
        u.contains("youtube.com") || u.contains("youtu.be") || u.contains("youtube-nocookie.com")
            || u.contains("tiktok.com")
            || u.contains("vimeo.com") || u.contains("player.vimeo.com")
            || u.contains("twitter.com") || u.contains("x.com")
            || ((u.contains("instagram.com") || u.contains("instagr.am")) && (
                u.contains("/reel/") || u.contains("/p/") || u.contains("/tv/") || u.contains("/share/")
                || (u.contains("/reels/") && !u.ends_with("/reels/") && !u.ends_with("/reels") && u.split("/reels/").nth(1).map(|s| !s.trim_matches('/').is_empty()).unwrap_or(false))
            ))
            || u.contains("facebook.com") || u.contains("fb.watch") || u.contains("fb.gg")
            || u.contains("dailymotion.com") || u.contains("dai.ly")
            || u.contains("bilibili.com") || u.contains("b23.tv")
            || u.contains("twitch.tv") || u.contains("clips.twitch.tv")
            || u.contains("reddit.com") || u.contains("v.redd.it")
            // --- Streaming & Live ---
            || u.contains("kick.com")
            || u.contains("rumble.com")
            || u.contains("odysee.com") || u.contains("lbry.tv")
            || u.contains("bitchute.com")
            || u.contains("rutube.ru")
            || u.contains("ok.ru") || u.contains("odnoklassniki.ru")
            || u.contains("vk.com") || u.contains("vk.video")
            || u.contains("coub.com")
            || u.contains("nimo.tv")
            // --- Audio Platforms ---
            || u.contains("soundcloud.com")
            || u.contains("bandcamp.com")
            || u.contains("audiomack.com")
            || u.contains("mixcloud.com")
            // --- News & Education ---
            || u.contains("ted.com")
            || u.contains("cnn.com")
            || u.contains("bbc.co.uk") || u.contains("bbc.com")
            || u.contains("aljazeera.com") || u.contains("aljazeera.net")
            // --- Asian Platforms ---
            || u.contains("nicovideo.jp") || u.contains("nico.ms")
            || u.contains("weibo.com")
            || u.contains("iqiyi.com")
            || u.contains("youku.com")
            || u.contains("v.qq.com")
            || u.contains("mgtv.com")
            // --- Arab Platforms ---
            || u.contains("shahid.mbc.net") || u.contains("shahid.net")
            || u.contains("cinemana.shabakaty.com")
            || u.contains("anghami.com")
            // --- File/Video Hosting ---
            || u.contains("streamable.com")
            || u.contains("vidio.com")
            || u.contains("loom.com")
            || u.contains("clipchamp.com")
            || u.contains("streamtape.com") || u.contains("streamlare.com")
            || u.contains("doodstream.com") || u.contains("dood.")
            || u.contains("vidoza.net")
            || u.contains("mp4upload.com")
            || u.contains("mixdrop.co") || u.contains("mixdrop.to")
            // --- Sports ---
            || u.contains("espn.com")
            || u.contains("mlb.com")
            // --- Misc Popular ---
            || u.contains("pinterest.com") || u.contains("pin.it")
            || u.contains("tumblr.com")
            || u.contains("imgur.com")
            || u.contains("9gag.com")
            || u.contains("linkedin.com")
            || u.contains("snapchat.com")
            || u.contains("threads.net")
    }

    /// Check if a URL or filename points to a direct downloadable file (not a streaming video).
    /// If it looks like a direct file, we use our multi-connection HTTP engine.
    /// If it looks like a streaming media platform, we try yt-dlp first.
    pub fn is_likely_direct_file(name_or_url: &str) -> bool {
        let u = name_or_url.to_lowercase();
        let path = u.split('?').next().unwrap_or(&u);
        let direct_exts = [
            ".zip", ".rar", ".7z", ".tar", ".gz", ".bz2", ".xz",
            ".exe", ".msi", ".dmg", ".deb", ".rpm", ".appimage",
            ".iso", ".img",
            ".pdf", ".doc", ".docx", ".xls", ".xlsx", ".ppt", ".pptx",
            ".apk", ".ipa", ".aab",
            ".torrent",
            ".bin", ".dat", ".cab",
        ];
        direct_exts.iter().any(|ext| path.ends_with(ext) || path.contains(ext))
    }

    /// Download video/audio via the extraction engine with real-time progress callbacks.
    ///
    /// If the first attempt fails (most often because YouTube changed something and the local
    /// yt-dlp is outdated), the extractor core is updated automatically and the download is
    /// retried once — the user no longer has to find and press "Update engine" manually.
    pub async fn download_media_stream(
        client: &Client,
        url: &str,
        dest_dir: &std::path::Path,
        format_id: Option<&str>,
        progress_tx: tokio::sync::mpsc::Sender<(f64, f64, u64, u64, f64, bool, Option<String>)>, // (progress 0..1, speed bps, downloaded bytes, total bytes, eta_secs, is_completed, final_filename)
        cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<std::path::PathBuf, String> {
        // YouTube needs a JavaScript runtime for its stream-URL challenges. Bootstrap Deno once if neither Deno nor Node is present.
        if Self::is_youtube_url(url) && Self::js_runtime_arg().is_none() {
            if let Err(e) = Self::ensure_deno_binary(client).await {
                record_engine_log(format!("[bootstrap] JS runtime unavailable: {}", e));
            }
        }

        let first = Self::run_ytdlp_download(client, url, dest_dir, format_id, progress_tx.clone(), cancel_rx.clone()).await;
        let first_err = match first {
            Ok(p) => return Ok(p),
            Err(e) => e,
        };
        if *cancel_rx.borrow() || first_err.contains("cancelled") {
            return Err(first_err);
        }

        record_engine_log(format!("[download] first attempt failed, auto-updating engine and retrying: {}", first_err));
        match Self::update_ytdlp_core(client).await {
            Ok(msg) => record_engine_log(format!("[update] {}", msg)),
            Err(e) => {
                record_engine_log(format!("[update] auto-update failed: {}", e));
                return Err(first_err);
            }
        }
        Self::run_ytdlp_download(client, url, dest_dir, format_id, progress_tx, cancel_rx)
            .await
            .map_err(|e| if e == first_err { e } else { format!("{} (after engine update; first attempt: {})", e, first_err) })
    }

    async fn kill_process_tree(child: &mut tokio::process::Child) {
        if let Some(pid) = child.id() {
            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                let mut tk = Command::new("taskkill");
                tk.args(["/F", "/T", "/PID", &pid.to_string()]);
                tk.creation_flags(0x08000000);
                let _ = tokio::task::spawn_blocking(move || {
                    let _ = tk.output();
                })
                .await;
            }
        }
        let _ = child.kill().await;
        let _ = child.wait().await;
    }

    async fn run_ytdlp_download(
        client: &Client,
        url: &str,
        dest_dir: &std::path::Path,
        format_id: Option<&str>,
        progress_tx: tokio::sync::mpsc::Sender<(f64, f64, u64, u64, f64, bool, Option<String>)>,
        mut cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<std::path::PathBuf, String> {
        use tokio::io::AsyncBufReadExt;

        let bin = match Self::ensure_ytdlp_binary(client).await {
            Ok(b) => b,
            Err(e) => Self::find_ytdlp_binary().ok_or_else(|| format!("Media extractor core not available: {}", e))?,
        };

        // YouTube (and most platforms) serve video and audio as SEPARATE streams for anything
        // above 360p, so FFmpeg is required for virtually every download. Always bootstrap it
        // when missing — not only for "high-res" format ids (format 397 = 480p was missed before,
        // leaving the user with unmerged `.f397.mp4` + `.f140.m4a` files).
        let mut has_ffmpeg = Self::find_ffmpeg_binary().is_some();
        if !has_ffmpeg {
            eprintln!("[SuperIDM] FFmpeg missing. Triggering On-Demand FFmpeg Auto-Bootstrap...");
            match Self::ensure_ffmpeg_binary(client).await {
                Ok(p) => {
                    eprintln!("[SuperIDM] FFmpeg Auto-Bootstrap succeeded: {:?}", p);
                    has_ffmpeg = true;
                }
                Err(e) => {
                    record_engine_log(format!("[bootstrap] FFmpeg unavailable, falling back to single combined stream: {}", e));
                }
            }
        }

        // Without FFmpeg we must NEVER request a `video+audio` combination, otherwise yt-dlp
        // leaves two unmerged part files. Only pick formats that already contain both tracks.
        const SINGLE_STREAM_ONLY: &str =
            "best[ext=mp4][vcodec!=none][acodec!=none]/best[vcodec!=none][acodec!=none]/best";

        let is_audio_only_req = format_id
            .map(|fid| {
                let fl = fid.to_lowercase();
                fl.contains("audio") || fl == "mp3"
            })
            .unwrap_or(false);

        let format_arg_str = match format_id {
            Some(fid) if !fid.is_empty() && fid != "best" && fid != "auto" => {
                if is_audio_only_req {
                    "bestaudio[ext=m4a][protocol^=http]/bestaudio[ext=m4a]/bestaudio/best".to_string()
                } else if has_ffmpeg {
                    if fid.contains('+') {
                        fid.to_string()
                    } else {
                        format!(
                            "{}+bestaudio[ext=m4a][protocol^=http]/{}+bestaudio[ext=m4a]/{}+bestaudio/best",
                            fid, fid, fid
                        )
                    }
                } else {
                    SINGLE_STREAM_ONLY.to_string()
                }
            }
            _ => {
                if has_ffmpeg {
                    "bestvideo[ext=mp4][protocol^=http]+bestaudio[ext=m4a][protocol^=http]/bestvideo[protocol^=http]+bestaudio[protocol^=http]/bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best[ext=mp4]/best".to_string()
                } else {
                    SINGLE_STREAM_ONLY.to_string()
                }
            }
        };
        let output_template = dest_dir.join("%(title)s.%(ext)s").to_string_lossy().to_string();

        let clean_url = Self::clean_target_url(url);

        let mut cmd = tokio::process::Command::new(bin);
        cmd.arg("-f")
            .arg(&format_arg_str)
            .arg("-o")
            .arg(&output_template)
            .arg("--no-playlist")
            .arg("--newline")
            .arg("--no-quiet")
            .arg("--no-simulate")
            .arg("--print")
            .arg("before_dl:SUPERIDM_SIZES:%(requested_formats.0.filesize,requested_formats.0.filesize_approx,filesize,filesize_approx)s:%(requested_formats.1.filesize,requested_formats.1.filesize_approx)s")
            .arg("--continue")
            // Parallel fragment downloading for DASH/HLS/fragmented streams
            .arg("--concurrent-fragments").arg("8")
            // Never hang forever on a dead socket; retry transient network errors instead.
            .arg("--socket-timeout").arg("30")
            .arg("--retries").arg("10")
            .arg("--fragment-retries").arg("10");

        if has_ffmpeg {
            if is_audio_only_req {
                cmd.arg("-x")
                    .arg("--audio-format").arg("mp3")
                    .arg("--audio-quality").arg("0");
            } else {
                cmd.arg("--merge-output-format").arg("mp4");
            }
        }

        // JS runtime for YouTube challenges (Deno preferred, explicit path so PATH is irrelevant)
        if let Some(rt) = Self::js_runtime_arg() {
            cmd.arg("--js-runtimes").arg(rt);
        }

        if let Some(ffmpeg_bin) = Self::find_ffmpeg_binary() {
            cmd.arg("--ffmpeg-location").arg(ffmpeg_bin.to_string_lossy().to_string());
            if let Some(parent_dir) = ffmpeg_bin.parent() {
                if let Ok(current_path) = std::env::var("PATH") {
                    cmd.env("PATH", format!("{};{}", parent_dir.to_string_lossy(), current_path));
                }
            }
        }

        cmd.arg(&clean_url);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn extractor: {}", e))?;
        let stdout = child.stdout.take().ok_or_else(|| "Failed to open stdout".to_string())?;
        let mut reader = tokio::io::BufReader::new(stdout).lines();

        // Asynchronously drain stderr to prevent pipe buffer deadlock on Windows
        let stderr = child.stderr.take();
        let stderr_log = std::sync::Arc::new(parking_lot::Mutex::new(Vec::<String>::new()));
        let stderr_log_c = stderr_log.clone();
        if let Some(err_stream) = stderr {
            tokio::spawn(async move {
                let mut err_reader = tokio::io::BufReader::new(err_stream).lines();
                while let Ok(Some(err_line)) = err_reader.next_line().await {
                    let mut log = stderr_log_c.lock();
                    if log.len() >= 40 {
                        log.remove(0);
                    }
                    log.push(err_line);
                }
            });
        }

        let mut is_audio_pass = false;
        let mut in_format_test = false;
        let mut dest_count = 0usize;
        let mut final_path: Option<std::path::PathBuf> = None;
        let mut total_bytes = 0u64;
        let mut video_bytes = 0u64;
        let mut audio_bytes = 0u64;
        let mut preknown_audio_bytes = 0u64;
        let mut max_progress = 0.0f64;
        let mut last_raw_percent = 0.0f64;

        loop {
            if *cancel_rx.borrow() {
                Self::kill_process_tree(&mut child).await;
                return Err("Download cancelled".into());
            }

            let line = tokio::select! {
                _ = cancel_rx.changed() => {
                    if *cancel_rx.borrow() {
                        Self::kill_process_tree(&mut child).await;
                        return Err("Download cancelled".into());
                    }
                    continue;
                }
                next = reader.next_line() => {
                    match next {
                        Ok(Some(l)) => l,
                        _ => break,
                    }
                }
            };

            let raw_line = line.trim();

            // Pre-flight exact combined size printed by --print before_dl:SUPERIDM_SIZES:<v>:<a>
            if let Some(rest) = raw_line.strip_prefix("SUPERIDM_SIZES:") {
                let mut parts = rest.split(':');
                let v_pre = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                let a_pre = parts.next().and_then(|s| s.trim().parse::<u64>().ok()).unwrap_or(0);
                if v_pre > 0 {
                    video_bytes = v_pre;
                }
                if a_pre > 0 {
                    audio_bytes = a_pre;
                    preknown_audio_bytes = a_pre;
                }
                total_bytes = video_bytes + audio_bytes;
                if total_bytes > 0 {
                    let _ = progress_tx.send((0.0, 0.0, 0, total_bytes, 0.0, false, None)).await;
                }
                continue;
            }

            // Ignore yt-dlp's internal 1-fragment HLS format test ("[info] Testing format ...")
            if raw_line.contains("[info] Testing format") {
                in_format_test = true;
                continue;
            }
            if raw_line.contains("Downloading 1 format(s):") {
                in_format_test = false;
                continue;
            }

            // Detect FFmpeg format merger or audio extraction destination
            if raw_line.contains("[Merger]") || raw_line.contains("Merging formats into") {
                if let Some(start) = raw_line.find('"') {
                    if let Some(end) = raw_line.rfind('"') {
                        if end > start {
                            let merged = &raw_line[start + 1..end];
                            final_path = Some(std::path::PathBuf::from(merged));
                        }
                    }
                }
                max_progress = max_progress.max(0.98);
                let _ = progress_tx.send((max_progress, 0.0, total_bytes, total_bytes, 1.0, false, None)).await;
                continue;
            }
            if raw_line.contains("[ExtractAudio]") {
                if let Some((_, dest_raw)) = raw_line.split_once("Destination:") {
                    let p = std::path::PathBuf::from(dest_raw.trim());
                    let fname = p.file_name().map(|n| n.to_string_lossy().to_string());
                    final_path = Some(p);
                    max_progress = max_progress.max(0.98);
                    let _ = progress_tx.send((max_progress, 0.0, total_bytes, total_bytes, 1.0, false, fname)).await;
                }
                continue;
            }

            // Detect already-downloaded file notice: "[download] <path> has already been downloaded"
            if let Some(before) = raw_line.strip_suffix("has already been downloaded") {
                let path_part = before.trim().strip_prefix("[download]").unwrap_or(before).trim();
                if !path_part.is_empty() {
                    final_path = Some(std::path::PathBuf::from(path_part));
                }
                continue;
            }

            // Detect stream destination: "[download] Destination: C:\...\Title.f313.webm"
            if let Some((_, dest_raw)) = raw_line.split_once("Destination:") {
                let dest = dest_raw.trim();
                let p = std::path::PathBuf::from(dest);
                // Skip yt-dlp's temporary format-test files (e.g. tmp0lusll5c.tmp)
                let is_probe_tmp = p.extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e.eq_ignore_ascii_case("tmp"))
                    .unwrap_or(false);
                if is_probe_tmp {
                    in_format_test = true;
                    continue;
                }
                in_format_test = false;
                dest_count += 1;
                if dest_count >= 2 {
                    is_audio_pass = true;
                } else if !is_audio_only_req {
                    if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                        // NOTE: Do NOT include "webm" here — 2K/4K YouTube video streams use .webm (VP9/AV1).
                        let ext_low = ext.to_lowercase();
                        if matches!(ext_low.as_str(), "m4a" | "opus" | "mp3" | "aac" | "ogg" | "flac" | "wav") {
                            is_audio_pass = true;
                        }
                    }
                }
                let is_temp_part = p.file_name()
                    .and_then(|n| n.to_str())
                    .and_then(|n| n.rsplit_once('.').map(|(stem, _)| stem))
                    .and_then(|stem| stem.rfind(".f").map(|i| &stem[i + 2..]))
                    .map(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '-'))
                    .unwrap_or(false);

                // Extract clean display filename so UI replaces "Video_Stream.mp4" immediately
                let target_ext = if is_audio_only_req && has_ffmpeg { "mp3" } else { "mp4" };
                let clean_display_name = p.file_name().and_then(|n| n.to_str()).map(|fname| {
                    if is_temp_part {
                        if let Some((stem, _)) = fname.rsplit_once('.') {
                            if let Some(f_idx) = stem.rfind(".f") {
                                return format!("{}.{}", &stem[..f_idx], target_ext);
                            }
                        }
                    } else if is_audio_only_req && has_ffmpeg {
                        if let Some((stem, _)) = fname.rsplit_once('.') {
                            return format!("{}.mp3", stem);
                        }
                    }
                    fname.to_string()
                });
                if dest_count == 1 && clean_display_name.is_some() {
                    let _ = progress_tx.send((max_progress, 0.0, 0, total_bytes, 0.0, false, clean_display_name)).await;
                }

                if !is_temp_part && final_path.is_none() {
                    final_path = Some(p);
                }
                continue;
            }

            if in_format_test || !raw_line.contains("[download]") {
                continue;
            }

            let sub_lines: Vec<&str> = raw_line.split("[download]").collect();

            for sub in sub_lines {
                let l = sub.trim();
                if l.is_empty() {
                    continue;
                }

                if l.contains('%') && l.contains("of") {
                    let tokens: Vec<&str> = l.split_whitespace().collect();
                    let mut raw_percent = 0.0f64;
                    let mut speed = 0.0f64;
                    let mut eta = 0.0f64;

                    for (i, tok) in tokens.iter().enumerate() {
                        if tok.ends_with('%') {
                            let p_str = tok.trim_end_matches('%');
                            raw_percent = p_str.parse::<f64>().unwrap_or(0.0) / 100.0;
                        }
                        if *tok == "of" && i + 1 < tokens.len() {
                            let is_approx = tokens[i + 1] == "~" || tokens[i + 1].starts_with('~');
                            let raw_size_tok = if tokens[i + 1] == "~" && i + 2 < tokens.len() {
                                tokens[i + 2]
                            } else {
                                tokens[i + 1]
                            };
                            if let Some(tb) = Self::parse_bytes_str(raw_size_tok) {
                                if is_audio_pass {
                                    if audio_bytes == 0 || !is_approx {
                                        audio_bytes = tb;
                                    }
                                } else if video_bytes == 0 || !is_approx {
                                    video_bytes = tb;
                                }
                                total_bytes = video_bytes + audio_bytes.max(preknown_audio_bytes);
                            }
                        }
                        if *tok == "at" && i + 1 < tokens.len() {
                            speed = Self::parse_speed_str(tokens[i + 1]);
                        }
                        if *tok == "ETA" && i + 1 < tokens.len() {
                            eta = Self::parse_eta_str(tokens[i + 1]);
                        }
                    }

                    // Auto-detect pass switch if raw_percent dropped drastically (e.g. 99% -> 2%)
                    if !is_audio_only_req && !is_audio_pass && last_raw_percent > 0.80 && raw_percent < 0.20 {
                        is_audio_pass = true;
                    }
                    last_raw_percent = raw_percent;

                    let current_dld = if is_audio_pass {
                        video_bytes + (audio_bytes.max(preknown_audio_bytes) as f64 * raw_percent.min(1.0)) as u64
                    } else {
                        (video_bytes as f64 * raw_percent.min(1.0)) as u64
                    };

                    let mut normalized_progress = if total_bytes > 0 && (is_audio_only_req || audio_bytes.max(preknown_audio_bytes) > 0) {
                        ((current_dld as f64) / (total_bytes as f64)).min(1.0) * 0.98
                    } else if is_audio_only_req {
                        raw_percent.min(1.0) * 0.95
                    } else if is_audio_pass {
                        0.88 + (raw_percent.min(1.0) * 0.10)
                    } else {
                        raw_percent.min(1.0) * 0.88
                    };

                    // Guarantee progress never retreats backwards
                    if normalized_progress < max_progress {
                        normalized_progress = max_progress;
                    } else {
                        max_progress = normalized_progress;
                    }

                    let _ = progress_tx.send((normalized_progress, speed, current_dld, total_bytes, eta, false, None)).await;
                }
            }
        }

        let status = tokio::select! {
            _ = cancel_rx.changed() => {
                if *cancel_rx.borrow() {
                    Self::kill_process_tree(&mut child).await;
                    return Err("Download cancelled".into());
                }
                child.wait().await.map_err(|e| format!("Child wait failed: {}", e))?
            }
            res = child.wait() => res.map_err(|e| format!("Child wait failed: {}", e))?,
        };
        if *cancel_rx.borrow() {
            return Err("Download cancelled".into());
        }
        if status.success() {
            // Auto-recovery: If split .f*.mp4 and .f*.m4a files exist, merge them directly with FFmpeg
            if let Some(merged_file) = Self::auto_merge_unmerged_parts(client, dest_dir).await {
                final_path = Some(merged_file);
            }

            let final_pb = match final_path {
                Some(fp) if fp.exists() => fp,
                _ => Self::find_newest_media_file(dest_dir).await.unwrap_or_else(|| dest_dir.to_path_buf()),
            };
            let disk_bytes = if let Ok(meta) = tokio::fs::metadata(&final_pb).await {
                meta.len()
            } else {
                0
            };
            let reported_total = if disk_bytes > 0 { disk_bytes } else { total_bytes };
            let fname = final_pb.file_name().map(|n| n.to_string_lossy().to_string());
            let _ = progress_tx.send((1.0, 0.0, reported_total, reported_total, 0.0, true, fname)).await;
            Ok(final_pb)
        } else {
            let last_err = {
                let log = stderr_log.lock();
                log.iter().rev()
                    .find(|l| l.contains("ERROR:") || l.contains("Error") || l.contains("error"))
                    .cloned()
                    .unwrap_or_else(|| log.last().cloned().unwrap_or_else(|| "Video extraction failed".to_string()))
            };
            let tail: Vec<String> = {
                let log = stderr_log.lock();
                log.iter().rev().take(8).rev().cloned().collect()
            };
            record_engine_log(format!(
                "[download] yt-dlp exit {:?} for {} | {}\n    stderr tail:\n    {}",
                status.code(),
                clean_url,
                last_err,
                tail.join("\n    ")
            ));
            Err(format!("Video extraction failed: {}", last_err))
        }
    }

    /// Find the newest media file downloaded in the destination directory
    pub async fn find_newest_media_file(dest_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut newest: Option<(std::path::PathBuf, std::time::SystemTime)> = None;
        if let Ok(mut entries) = tokio::fs::read_dir(dest_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    let ext_low = ext.to_lowercase();
                    if ext_low == "mp4" || ext_low == "mkv" || ext_low == "webm" || ext_low == "mp3" || ext_low == "m4a" {
                        if let Ok(meta) = entry.metadata().await {
                            if let Ok(modified) = meta.modified() {
                                if newest.as_ref().map(|(_, t)| modified > *t).unwrap_or(true) {
                                    newest = Some((path, modified));
                                }
                            }
                        }
                    }
                }
            }
        }
        newest.map(|(p, _)| p)
    }

    /// Automatically find matching video and audio part files in dest_dir and merge them with FFmpeg.
    pub async fn auto_merge_unmerged_parts(client: &Client, dest_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut video_parts = Vec::new();
        let mut audio_parts = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(dest_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    // Only genuine yt-dlp format parts: "<title>.f<digits>.<ext>"
                    let is_part = |exts: &[&str]| -> bool {
                        let lower = name.to_lowercase();
                        exts.iter().any(|ext| {
                            lower.strip_suffix(&format!(".{}", ext))
                                .and_then(|stem| stem.rfind(".f").map(|i| &stem[i + 2..]))
                                .map(|digits| !digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit() || c == '-'))
                                .unwrap_or(false)
                        })
                    };
                    if is_part(&["mp4"]) {
                        video_parts.push(path);
                    } else if is_part(&["m4a", "webm", "opus"]) {
                        audio_parts.push(path);
                    }
                }
            }
        }

        if video_parts.is_empty() || audio_parts.is_empty() {
            return None;
        }

        // Parts exist but FFmpeg may be missing (e.g. bootstrap failed earlier) — retry it now.
        let ffmpeg_bin = match Self::find_ffmpeg_binary() {
            Some(b) => b,
            None => Self::ensure_ffmpeg_binary(client).await.ok()?,
        };

        for vpath in &video_parts {
            let vname = match vpath.file_name().and_then(|n| n.to_str()) {
                Some(n) => n,
                None => continue,
            };
            let base_name = match vname.rfind(".f") {
                Some(idx) => &vname[..idx],
                None => continue,
            };
            let audio_prefix = format!("{}.f", base_name);

            if let Some(apath) = audio_parts.iter().find(|ap| {
                ap.file_name().and_then(|n| n.to_str()).map(|an| an.starts_with(&audio_prefix)).unwrap_or(false)
            }) {
                let final_merged = dest_dir.join(format!("{}.mp4", base_name));
                let mut cmd = tokio::process::Command::new(&ffmpeg_bin);
                cmd.arg("-y")
                    .arg("-i").arg(vpath)
                    .arg("-i").arg(apath)
                    .arg("-map").arg("0:v:0")
                    .arg("-map").arg("1:a:0")
                    .arg("-c").arg("copy")
                    .arg(&final_merged)
                    .stdout(std::process::Stdio::null())
                    .stderr(std::process::Stdio::null());

                #[cfg(target_os = "windows")]
                {
                    cmd.creation_flags(0x08000000);
                }

                if let Ok(status) = cmd.status().await {
                    if status.success() {
                        let _ = tokio::fs::remove_file(vpath).await;
                        let _ = tokio::fs::remove_file(apath).await;
                        return Some(final_merged);
                    }
                }
            }
        }
        None
    }

    fn parse_bytes_str(s: &str) -> Option<u64> {
        let clean = s.trim().trim_start_matches('~').trim().to_lowercase();
        let num_str = clean
            .trim_start_matches(|c: char| !c.is_numeric() && c != '.')
            .trim_end_matches(|c: char| !c.is_numeric() && c != '.');
        let val: f64 = num_str.parse().ok()?;
        if clean.ends_with("gib") || clean.ends_with("gb") {
            Some((val * 1024.0 * 1024.0 * 1024.0) as u64)
        } else if clean.ends_with("mib") || clean.ends_with("mb") {
            Some((val * 1024.0 * 1024.0) as u64)
        } else if clean.ends_with("kib") || clean.ends_with("kb") {
            Some((val * 1024.0) as u64)
        } else {
            Some(val as u64)
        }
    }

    fn parse_speed_str(s: &str) -> f64 {
        let clean = s.trim().trim_start_matches('~').trim().to_lowercase();
        let num_str = clean
            .trim_start_matches(|c: char| !c.is_numeric() && c != '.')
            .trim_end_matches(|c: char| !c.is_numeric() && c != '.');
        let val: f64 = num_str.parse().unwrap_or(0.0);
        if clean.contains("gib/s") || clean.contains("gb/s") {
            val * 1024.0 * 1024.0 * 1024.0
        } else if clean.contains("mib/s") || clean.contains("mb/s") {
            val * 1024.0 * 1024.0
        } else if clean.contains("kib/s") || clean.contains("kb/s") {
            val * 1024.0
        } else {
            val
        }
    }

    fn parse_eta_str(s: &str) -> f64 {
        let parts: Vec<&str> = s.trim().split(':').collect();
        match parts.len() {
            1 => parts[0].parse().unwrap_or(0.0),
            2 => {
                let m: f64 = parts[0].parse().unwrap_or(0.0);
                let sec: f64 = parts[1].parse().unwrap_or(0.0);
                m * 60.0 + sec
            }
            3 => {
                let h: f64 = parts[0].parse().unwrap_or(0.0);
                let m: f64 = parts[1].parse().unwrap_or(0.0);
                let sec: f64 = parts[2].parse().unwrap_or(0.0);
                h * 3600.0 + m * 60.0 + sec
            }
            _ => 0.0,
        }
    }

    /// Extract media information and available formats from any webpage or video URL.
    pub async fn extract_info(client: &Client, url: &str) -> Result<MediaExtractionInfo, String> {
        let clean_url_str = Self::clean_target_url(url);
        let clean_url = clean_url_str.as_str();

        // 1. Cinemana Shabakaty special extractor
        if clean_url.contains("cinemana.shabakaty.com") {
            if let Ok(info) = Self::extract_cinemana_info(client, clean_url).await {
                return Ok(info);
            }
        }

        // 2. Ensure yt-dlp binary is available on machine (auto-bootstrapping for clean Windows)
        let _ = Self::ensure_ytdlp_binary(client).await;
        if Self::is_youtube_url(clean_url) && Self::js_runtime_arg().is_none() {
            let _ = Self::ensure_deno_binary(client).await;
        }

        // 3. Try yt-dlp if available (supports 1000+ complex platforms including YouTube, TikTok, Vimeo)
        match Self::extract_with_ytdlp(clean_url) {
            Ok(info) => return Ok(info),
            Err(e) => {
                // If it's a known video platform, return the actual extraction error — don't fall back to HTML sniffer
                if Self::is_platform_url(clean_url) {
                    return Err(e);
                }
            }
        }

        // 3. Native HTML & Media Sniffer Fallback
        Self::extract_with_native_sniffer(client, clean_url).await
    }

    /// Extract items in a video/media playlist (e.g. YouTube playlist, channel, course, album)
    pub async fn extract_playlist_info(client: &Client, url: &str) -> Result<PlaylistExtractionInfo, String> {
        let clean_url_str = Self::clean_target_url(url);
        let clean_url = clean_url_str.as_str();

        let _ = Self::ensure_ytdlp_binary(client).await;
        if Self::is_youtube_url(clean_url) && Self::js_runtime_arg().is_none() {
            let _ = Self::ensure_deno_binary(client).await;
        }
        let bin = Self::find_ytdlp_binary().ok_or_else(|| "yt-dlp binary not found on system".to_string())?;

        let mut cmd = Command::new(bin);
        cmd.arg("-J")
            .arg("--flat-playlist")
            .arg("--no-warnings");
        if !Self::is_youtube_url(clean_url) {
            cmd.arg("--user-agent").arg(DEFAULT_USER_AGENT);
        }
        if let Some(rt) = Self::js_runtime_arg() {
            cmd.arg("--js-runtimes").arg(rt);
        }
        cmd.arg(&clean_url);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let output = cmd
            .output()
            .map_err(|e| format!("Failed to launch yt-dlp: {}", e))?;

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            let first_line = err_msg
                .lines()
                .rev()
                .find(|l| {
                    let low = l.to_lowercase();
                    low.contains("error:") || low.contains("error ") || low.contains("[pyi-")
                })
                .or_else(|| err_msg.lines().rev().find(|l| !l.trim().is_empty()))
                .unwrap_or("Playlist unavailable or invalid URL")
                .trim();
            record_engine_log(format!("[playlist] extraction failed for {} | {}", clean_url, first_line));
            return Err(first_line.to_string());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let val: Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Invalid yt-dlp playlist JSON: {}", e))?;

        let id = val.get("id").and_then(|v| v.as_str()).unwrap_or("playlist").to_string();
        let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("Playlist").to_string();
        let uploader = val.get("uploader").or_else(|| val.get("channel")).and_then(|v| v.as_str()).map(|s| s.to_string());

        let mut entries = Vec::new();
        if let Some(arr) = val.get("entries").and_then(|v| v.as_array()) {
            for (idx, item) in arr.iter().enumerate() {
                let item_id = item.get("id").and_then(|v| v.as_str()).unwrap_or(&format!("item_{}", idx + 1)).to_string();
                let item_title = item.get("title").and_then(|v| v.as_str()).unwrap_or(&format!("Video {}", idx + 1)).to_string();
                
                let raw_item_url = item.get("url").and_then(|v| v.as_str());
                let item_url = if let Some(u) = raw_item_url {
                    if u.starts_with("http") {
                        u.to_string()
                    } else if u.len() == 11 && !u.contains('/') {
                        format!("https://www.youtube.com/watch?v={}", u)
                    } else {
                        format!("https://www.youtube.com/watch?v={}", item_id)
                    }
                } else if !item_id.is_empty() {
                    format!("https://www.youtube.com/watch?v={}", item_id)
                } else {
                    continue;
                };

                let duration_secs = item.get("duration").and_then(|v| v.as_u64());
                let duration_string = duration_secs.map(|d| {
                    let hrs = d / 3600;
                    let mins = (d % 3600) / 60;
                    let secs = d % 60;
                    if hrs > 0 {
                        format!("{:02}:{:02}:{:02}", hrs, mins, secs)
                    } else {
                        format!("{:02}:{:02}", mins, secs)
                    }
                });

                let thumbnail = item.get("thumbnails")
                    .and_then(|t| t.as_array())
                    .and_then(|arr| arr.last())
                    .and_then(|t| t.get("url"))
                    .and_then(|u| u.as_str())
                    .or_else(|| item.get("thumbnail").and_then(|u| u.as_str()))
                    .map(|s| s.to_string());

                let channel = item.get("channel")
                    .or_else(|| item.get("uploader"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                entries.push(PlaylistItem {
                    id: item_id,
                    title: item_title,
                    url: item_url,
                    duration_secs,
                    duration_string,
                    thumbnail,
                    uploader: channel,
                });
            }
        } else if val.get("_type").and_then(|v| v.as_str()) != Some("playlist") {
            let duration_secs = val.get("duration").and_then(|v| v.as_u64());
            let duration_string = duration_secs.map(|d| {
                let hrs = d / 3600;
                let mins = (d % 3600) / 60;
                let secs = d % 60;
                if hrs > 0 {
                    format!("{:02}:{:02}:{:02}", hrs, mins, secs)
                } else {
                    format!("{:02}:{:02}", mins, secs)
                }
            });
            let thumbnail = val.get("thumbnail").and_then(|v| v.as_str()).map(|s| s.to_string());
            entries.push(PlaylistItem {
                id: id.clone(),
                title: title.clone(),
                url: clean_url.to_string(),
                duration_secs,
                duration_string,
                thumbnail,
                uploader: uploader.clone(),
            });
        }

        let count = entries.len();
        if count == 0 {
            return Err("No playable videos found in this playlist or URL".to_string());
        }

        Ok(PlaylistExtractionInfo {
            id,
            title,
            uploader,
            count,
            entries,
        })
    }

    /// Cinemana Shabakaty direct API video file extractor
    pub async fn extract_cinemana_info(client: &Client, url: &str) -> Result<MediaExtractionInfo, String> {
        let target_id = if let Some(idx) = url.find("lastEpisodeVideoID=") {
            let after = &url[idx + 19..];
            after.split('&').next().unwrap_or("").to_string()
        } else if let Some(idx) = url.find("/video/ar/") {
            let after = &url[idx + 10..];
            after.split('?').next().unwrap_or("").to_string()
        } else if let Some(idx) = url.find("/video/en/") {
            let after = &url[idx + 10..];
            after.split('?').next().unwrap_or("").to_string()
        } else {
            return Err("Could not parse Cinemana video ID".into());
        };

        if target_id.is_empty() {
            return Err("Empty Cinemana video ID".into());
        }

        let api_url = format!("https://cinemana.shabakaty.com/api/android/videoFiles/id/{}", target_id);
        let resp = client
            .get(&api_url)
            .header("User-Agent", "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36")
            .header("Accept", "application/json, text/plain, */*")
            .header("Referer", "https://cinemana.shabakaty.com/")
            .send()
            .await
            .map_err(|e| format!("Cinemana API request failed: {}", e))?;

        let json: Value = resp.json().await.map_err(|e| format!("Invalid JSON from Cinemana: {}", e))?;

        let mut formats = Vec::new();
        if let Some(arr) = json.as_array() {
            for item in arr {
                let res = item.get("resolution").and_then(|v| v.as_str()).unwrap_or("HD");
                let v_url = item.get("videoUrl")
                    .or_else(|| item.get("file"))
                    .or_else(|| item.get("url"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let container = item.get("container").and_then(|v| v.as_str()).unwrap_or("mp4");

                if !v_url.is_empty() {
                    formats.push(MediaFormat {
                        format_id: format!("cinemana_{}", res),
                        ext: container.to_string(),
                        resolution: res.to_string(),
                        note: "Shabakaty CDN".into(),
                        filesize: None,
                        vcodec: None,
                        acodec: None,
                        url: Some(v_url.to_string()),
                    });
                }
            }
        }

        // Also fetch title from the main webpage if possible
        let mut title = "Cinemana Video".to_string();
        if let Ok(page_resp) = client.get(url).header("User-Agent", DEFAULT_USER_AGENT).send().await {
            if let Ok(html) = page_resp.text().await {
                if let Some(t) = Self::extract_meta_or_title(&html) {
                    title = t.replace(" - سينمانا", "").replace(" - Cinemana", "").trim().to_string();
                }
            }
        }

        if formats.is_empty() {
            return Err("No video streams found in Cinemana API".into());
        }

        let is_stream = formats.iter().any(|f| f.ext == "m3u8" || f.ext == "mpd");

        Ok(MediaExtractionInfo {
            url: url.to_string(),
            title,
            thumbnail: None,
            duration_secs: None,
            uploader: Some("Shabakaty Cinemana".into()),
            formats,
            is_stream,
        })
    }

    /// Invoke `yt-dlp` CLI to extract clean JSON metadata.
    fn extract_with_ytdlp(url: &str) -> Result<MediaExtractionInfo, String> {
        let clean_url = Self::clean_target_url(url);
        let bin = Self::find_ytdlp_binary().ok_or_else(|| "yt-dlp binary not found on system".to_string())?;
        let rt_arg = Self::js_runtime_arg();

        let run_once = |exe: &std::path::Path, js_rt: Option<&str>| -> Result<std::process::Output, String> {
            let mut cmd = Command::new(exe);
            cmd.arg("-J")
                .arg("--no-playlist")
                .arg("--no-warnings")
                .arg("--socket-timeout")
                .arg("15");
            if !Self::is_youtube_url(&clean_url) {
                cmd.arg("--user-agent").arg(DEFAULT_USER_AGENT);
            }
            if let Some(rt) = js_rt {
                cmd.arg("--js-runtimes").arg(rt);
            }
            cmd.arg(&clean_url);

            #[cfg(target_os = "windows")]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
            }

            cmd.output().map_err(|e| format!("Failed to launch yt-dlp: {}", e))
        };

        let mut output = run_once(&bin, rt_arg.as_deref())?;

        // If first attempt failed, retry once without --js-runtimes (or with bundled binary if local failed)
        if !output.status.success() {
            let fallback_bin = Self::find_bundled_file("yt-dlp.exe", 15_000_000).unwrap_or_else(|| bin.clone());
            if rt_arg.is_some() || fallback_bin != bin {
                if let Ok(retry_out) = run_once(&fallback_bin, None) {
                    if retry_out.status.success() {
                        output = retry_out;
                    }
                }
            }
        }

        if !output.status.success() {
            let err_msg = String::from_utf8_lossy(&output.stderr);
            let first_line = err_msg
                .lines()
                .rev()
                .find(|l| {
                    let low = l.to_lowercase();
                    low.contains("error:") || low.contains("error ") || low.contains("[pyi-")
                })
                .or_else(|| err_msg.lines().rev().find(|l| !l.trim().is_empty()))
                .unwrap_or("Video unavailable or invalid URL")
                .trim();
            record_engine_log(format!("[extract] failed for {} | {}", clean_url, first_line));
            return Err(first_line.to_string());
        }

        let json_str = String::from_utf8_lossy(&output.stdout);
        let val: Value = serde_json::from_str(&json_str)
            .map_err(|e| format!("Invalid yt-dlp JSON: {}", e))?;

        let title = val.get("title").and_then(|v| v.as_str()).unwrap_or("Media").to_string();
        let thumbnail = val.get("thumbnail").and_then(|v| v.as_str()).map(|s| s.to_string());
        let duration_secs = val.get("duration").and_then(|v| v.as_u64());
        let uploader = val.get("uploader").and_then(|v| v.as_str()).map(|s| s.to_string());

        let mut formats = Vec::new();
        if let Some(fmts) = val.get("formats").and_then(|v| v.as_array()) {
            for f in fmts {
                let format_id = f.get("format_id").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let ext = f.get("ext").and_then(|v| v.as_str()).unwrap_or("mp4").to_string();
                let resolution = f.get("resolution")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
                    .or_else(|| {
                        let h = f.get("height").and_then(|v| v.as_u64());
                        h.map(|h_val| format!("{}p", h_val))
                    })
                    .unwrap_or_else(|| "audio only".to_string());

                let note = f.get("format_note").and_then(|v| v.as_str()).unwrap_or("").to_string();
                let filesize = f.get("filesize").and_then(|v| v.as_u64())
                    .or_else(|| f.get("filesize_approx").and_then(|v| v.as_u64()));
                let vcodec = f.get("vcodec").and_then(|v| v.as_str()).map(|s| s.to_string());
                let acodec = f.get("acodec").and_then(|v| v.as_str()).map(|s| s.to_string());
                let direct_url = f.get("url").and_then(|v| v.as_str()).map(|s| s.to_string());

                if ext == "mhtml" || ext == "webp" || ext == "jpg" || ext == "jpeg" || ext == "png" || format_id.starts_with("sb") || resolution.contains("storyboard") {
                    continue;
                }

                if !format_id.is_empty() {
                    formats.push(MediaFormat {
                        format_id,
                        ext,
                        resolution,
                        note,
                        filesize,
                        vcodec,
                        acodec,
                        url: direct_url,
                    });
                }
            }
        }

        let is_stream = formats.iter().any(|f| f.ext == "m3u8" || f.ext == "mpd");

        Ok(MediaExtractionInfo {
            url: url.to_string(),
            title,
            thumbnail,
            duration_secs,
            uploader,
            formats,
            is_stream,
        })
    }

    /// Native sniffer for direct media, HTML5 video tags, player iframes, and m3u8/mp4 stream manifests.
    async fn extract_with_native_sniffer(client: &Client, url: &str) -> Result<MediaExtractionInfo, String> {
        let resp = client
            .get(url)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch URL: {}", e))?;

        let final_url = resp.url().to_string();
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_lowercase();

        // 1. If direct video or audio stream
        if content_type.contains("video/") || content_type.contains("audio/")
            || final_url.ends_with(".mp4") || final_url.ends_with(".mkv")
            || final_url.ends_with(".webm") || final_url.ends_with(".avi")
        {
            let filename = final_url.split('/').last().unwrap_or("video.mp4").split('?').next().unwrap_or("video.mp4");
            return Ok(MediaExtractionInfo {
                url: final_url.clone(),
                title: filename.to_string(),
                thumbnail: None,
                duration_secs: None,
                uploader: None,
                formats: vec![MediaFormat {
                    format_id: "direct".into(),
                    ext: "mp4".into(),
                    resolution: "Direct Stream (Original)".into(),
                    note: "Direct Video".into(),
                    filesize: None,
                    vcodec: None,
                    acodec: None,
                    url: Some(final_url),
                }],
                is_stream: false,
            });
        }

        // 2. If direct HLS/DASH manifest
        if final_url.contains(".m3u8") || content_type.contains("application/x-mpegurl") || content_type.contains("vnd.apple.mpegurl") {
            return Ok(MediaExtractionInfo {
                url: final_url.clone(),
                title: "HLS Stream".to_string(),
                thumbnail: None,
                duration_secs: None,
                uploader: None,
                formats: vec![MediaFormat {
                    format_id: "hls".into(),
                    ext: "m3u8".into(),
                    resolution: "HLS Adaptive Stream".into(),
                    note: "Auto Master".into(),
                    filesize: None,
                    vcodec: None,
                    acodec: None,
                    url: Some(final_url),
                }],
                is_stream: true,
            });
        }

        // 3. Deep HTML & Embedded Player Parsing
        let body = resp.text().await.map_err(|e| format!("Failed to read HTML body: {}", e))?;
        let title = Self::extract_meta_or_title(&body).unwrap_or_else(|| "Media Stream".to_string());
        let thumbnail = Self::extract_meta_property(&body, "og:image");

        let mut formats = Vec::new();

        // Check HLS .m3u8
        if let Some(m3u8_url) = Self::find_stream_url_in_html(&body, ".m3u8", &final_url) {
            formats.push(MediaFormat {
                format_id: "hls_stream".into(),
                ext: "m3u8".into(),
                resolution: "HLS Adaptive Stream".into(),
                note: "Master Playlist".into(),
                filesize: None,
                vcodec: None,
                acodec: None,
                url: Some(m3u8_url),
            });
        }

        // Check direct .mp4 in HTML/JS
        if let Some(mp4_url) = Self::find_stream_url_in_html(&body, ".mp4", &final_url) {
            formats.push(MediaFormat {
                format_id: "direct_mp4".into(),
                ext: "mp4".into(),
                resolution: "HD Stream".into(),
                note: "Direct Video".into(),
                filesize: None,
                vcodec: None,
                acodec: None,
                url: Some(mp4_url),
            });
        }

        // Check OpenGraph video
        if let Some(og_vid) = Self::extract_meta_property(&body, "og:video") {
            if !formats.iter().any(|f| f.url.as_deref() == Some(&og_vid)) {
                formats.push(MediaFormat {
                    format_id: "og_video".into(),
                    ext: "mp4".into(),
                    resolution: "Web Stream".into(),
                    note: "Embedded Video".into(),
                    filesize: None,
                    vcodec: None,
                    acodec: None,
                    url: Some(og_vid),
                });
            }
        }

        // Check Player Iframes (e.g. streaming site embeds)
        if formats.is_empty() {
            if let Some(iframe_src) = Self::find_player_iframe(&body, &final_url) {
                if let Ok(iframe_resp) = client.get(&iframe_src).header("User-Agent", DEFAULT_USER_AGENT).header("Referer", &final_url).send().await {
                    if let Ok(iframe_body) = iframe_resp.text().await {
                        if let Some(m3u8_url) = Self::find_stream_url_in_html(&iframe_body, ".m3u8", &iframe_src) {
                            formats.push(MediaFormat {
                                format_id: "iframe_hls".into(),
                                ext: "m3u8".into(),
                                resolution: "HLS Adaptive Stream".into(),
                                note: "Embedded Player".into(),
                                filesize: None,
                                vcodec: None,
                                acodec: None,
                                url: Some(m3u8_url),
                            });
                        } else if let Some(mp4_url) = Self::find_stream_url_in_html(&iframe_body, ".mp4", &iframe_src) {
                            formats.push(MediaFormat {
                                format_id: "iframe_mp4".into(),
                                ext: "mp4".into(),
                                resolution: "HD Stream".into(),
                                note: "Embedded Player".into(),
                                filesize: None,
                                vcodec: None,
                                acodec: None,
                                url: Some(mp4_url),
                            });
                        }
                    }
                }
            }
        }

        if formats.is_empty() {
            return Err("This is a web page URL without direct video links. Please play the video in your browser and use the SuperIDM download overlay, or paste a direct video link.".to_string());
        }

        let is_stream = formats.iter().any(|f| f.ext == "m3u8" || f.ext == "mpd");

        Ok(MediaExtractionInfo {
            url: url.to_string(),
            title,
            thumbnail,
            duration_secs: None,
            uploader: None,
            formats,
            is_stream,
        })
    }

    fn find_player_iframe(html: &str, base_url: &str) -> Option<String> {
        let mut search_from = 0;
        while let Some(pos) = html[search_from..].find("<iframe") {
            let abs_pos = search_from + pos;
            let window = &html[abs_pos..abs_pos.min(html.len()) + 500.min(html.len() - abs_pos)];
            if let Some(src_pos) = window.find("src=") {
                let quote = window[src_pos + 4..].chars().next()?;
                if quote == '"' || quote == '\'' {
                    let start = src_pos + 5;
                    if let Some(end) = window[start..].find(quote) {
                        let raw_src = &window[start..start + end];
                        if !raw_src.is_empty() && !raw_src.starts_with("about:") && !raw_src.starts_with("javascript:") {
                            if raw_src.starts_with("http://") || raw_src.starts_with("https://") {
                                return Some(raw_src.to_string());
                            } else if raw_src.starts_with("//") {
                                return Some(format!("https:{}", raw_src));
                            } else if raw_src.starts_with('/') {
                                if let Ok(base) = reqwest::Url::parse(base_url) {
                                    if let Some(host) = base.host_str() {
                                        return Some(format!("{}://{}{}", base.scheme(), host, raw_src));
                                    }
                                }
                            }
                        }
                    }
                }
            }
            search_from = abs_pos + 7;
        }
        None
    }

    fn extract_meta_or_title(html: &str) -> Option<String> {
        if let Some(t) = Self::extract_meta_property(html, "og:title") {
            return Some(t);
        }
        if let Some(start) = html.find("<title>") {
            let rest = &html[start + 7..];
            if let Some(end) = rest.find("</title>") {
                return Some(rest[..end].trim().to_string());
            }
        }
        None
    }

    fn extract_meta_property(html: &str, property: &str) -> Option<String> {
        let pattern = format!("property=\"{}\"", property);
        let pattern_alt = format!("property='{}'", property);

        let pos = html.find(&pattern).or_else(|| html.find(&pattern_alt))?;
        let window = &html[pos..pos.min(html.len()) + 400.min(html.len() - pos)];

        if let Some(content_pos) = window.find("content=\"") {
            let start = content_pos + 9;
            let rest = &window[start..];
            if let Some(end) = rest.find('"') {
                return Some(rest[..end].to_string());
            }
        } else if let Some(content_pos) = window.find("content='") {
            let start = content_pos + 9;
            let rest = &window[start..];
            if let Some(end) = rest.find('\'') {
                return Some(rest[..end].to_string());
            }
        }
        None
    }

    fn find_stream_url_in_html(html: &str, extension: &str, base_url: &str) -> Option<String> {
        let mut search_from = 0;
        while let Some(pos) = html[search_from..].find(extension) {
            let abs_pos = search_from + pos;
            let start_slice = &html[..abs_pos];
            let start = start_slice.rfind('"').or_else(|| start_slice.rfind('\'')).unwrap_or(0) + 1;
            let end = abs_pos + extension.len();

            let matched = &html[start..end];
            if matched.starts_with("http://") || matched.starts_with("https://") {
                return Some(matched.to_string());
            } else if matched.starts_with("//") {
                return Some(format!("https:{}", matched));
            } else if matched.starts_with('/') {
                if let Ok(base) = reqwest::Url::parse(base_url) {
                    if let Some(host) = base.host_str() {
                        return Some(format!("{}://{}{}", base.scheme(), host, matched));
                    }
                }
            }
            search_from = abs_pos + extension.len();
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_target_url_markdown() {
        let input = "[Creatina para tus Músculos o Cerebro? | Casi Creativo](https://www.youtube.com/watch?v=ew0RSjgDgH8)";
        assert_eq!(
            MediaExtractor::clean_target_url(input),
            "https://www.youtube.com/watch?v=ew0RSjgDgH8"
        );
    }

    #[test]
    fn test_clean_target_url_surrounding_text() {
        let input = "Check this video: https://www.youtube.com/watch?v=ew0RSjgDgH8 and let me know";
        assert_eq!(
            MediaExtractor::clean_target_url(input),
            "https://www.youtube.com/watch?v=ew0RSjgDgH8"
        );
    }

    #[test]
    fn test_clean_target_url_plain() {
        let input = "https://www.youtube.com/watch?v=ew0RSjgDgH8";
        assert_eq!(
            MediaExtractor::clean_target_url(input),
            "https://www.youtube.com/watch?v=ew0RSjgDgH8"
        );
    }

    #[test]
    fn test_clean_target_url_double_pasted() {
        let input = "https://www.youtube.com/watch?v=yd8jh9QYFEs&list=RDyd8jh9QYFEs&start_radio=1https://www.youtube.com/watch?v=yd8jh9QYFEs&list=RDyd8jh9QYFEs&start_radio=1";
        assert_eq!(
            MediaExtractor::clean_target_url(input),
            "https://www.youtube.com/watch?v=yd8jh9QYFEs&list=RDyd8jh9QYFEs&start_radio=1"
        );
    }

    #[test]
    fn test_verify_bundled_binaries() {
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let msix_dir = manifest_dir.parent().unwrap().join("msix_build");
        let ytdlp = msix_dir.join("yt-dlp.exe");
        if ytdlp.exists() {
            let ver = MediaExtractor::verify_ytdlp_binary(&ytdlp);
            assert!(ver.is_some(), "Bundled yt-dlp.exe should pass verification");
            assert!(ver.unwrap().as_str() >= "2026.10.01");
        }
        // Small file should be rejected by verify_ytdlp_binary
        let cargo_toml = manifest_dir.join("Cargo.toml");
        assert!(MediaExtractor::verify_ytdlp_binary(&cargo_toml).is_none());
    }

    #[test]
    fn test_engine_status_and_log_buffer() {
        record_engine_log("[test] diagnostic log entry");
        let logs = recent_engine_log();
        assert!(logs.iter().any(|l| l.contains("[test] diagnostic log entry")));

        let status = MediaExtractor::engine_status();
        assert!(status.ytdlp_version.is_some());
        assert!(status.ffmpeg_ok);
        assert!(status.js_ok);
        assert!(status.ready);
    }
}
