use std::path::PathBuf;
use std::process::Command;
use reqwest::Client;
use serde_json::Value;

use super::session::DEFAULT_USER_AGENT;
use super::types::{MediaExtractionInfo, MediaFormat};

/// Universal Media Extractor Pipeline.
pub struct MediaExtractor;

impl MediaExtractor {
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

    /// Locate the yt-dlp binary across PATH, Python Scripts, and WinGet directories.
    fn find_ytdlp_binary() -> Option<PathBuf> {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new("yt-dlp");
            cmd.arg("--version");
            cmd.creation_flags(0x08000000);
            if let Ok(output) = cmd.output() {
                if output.status.success() {
                    return Some(PathBuf::from("yt-dlp"));
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Ok(output) = Command::new("yt-dlp").arg("--version").output() {
                if output.status.success() {
                    return Some(PathBuf::from("yt-dlp"));
                }
            }
        }

        if let Some(home) = dirs::home_dir() {
            let candidates = vec![
                home.join(r"AppData\Local\Programs\Python\Python314\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Local\Programs\Python\Python313\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Local\Programs\Python\Python312\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Local\Programs\Python\Python311\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Local\Programs\Python\Python310\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Roaming\Python\Scripts\yt-dlp.exe"),
                home.join(r"AppData\Local\Microsoft\WinGet\Links\yt-dlp.exe"),
                home.join(r"AppData\Local\bin\yt-dlp.exe"),
                home.join(r".local\bin\yt-dlp.exe"),
            ];
            for path in candidates {
                if path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    /// Locate ffmpeg binary on the system with full absolute path.
    pub fn find_ffmpeg_binary() -> Option<PathBuf> {
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

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            let mut cmd = Command::new("where");
            cmd.arg("ffmpeg.exe");
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

        None
    }

    /// Check if a URL is from a known video/media platform that requires signature/token extraction.
    pub fn is_platform_url(url: &str) -> bool {
        let u = url.to_lowercase();
        u.contains("youtube.com")
            || u.contains("youtu.be")
            || u.contains("tiktok.com")
            || u.contains("vimeo.com")
            || u.contains("twitter.com")
            || u.contains("x.com")
            || u.contains("instagram.com")
            || u.contains("facebook.com")
            || u.contains("fb.watch")
            || u.contains("dailymotion.com")
            || u.contains("bilibili.com")
            || u.contains("twitch.tv")
            || u.contains("reddit.com")
    }

    /// Download video/audio via the extraction engine with real-time progress callbacks.
    pub async fn download_media_stream(
        url: &str,
        dest_dir: &std::path::Path,
        format_id: Option<&str>,
        progress_tx: tokio::sync::mpsc::Sender<(f64, f64, u64, u64, f64, bool, Option<String>)>, // (progress 0..1, speed bps, downloaded bytes, total bytes, eta_secs, is_completed, final_filename)
        cancel_rx: tokio::sync::watch::Receiver<bool>,
    ) -> Result<std::path::PathBuf, String> {
        use tokio::io::AsyncBufReadExt;

        let bin = Self::find_ytdlp_binary().ok_or_else(|| "yt-dlp binary not found".to_string())?;

        let format_arg_str = match format_id {
            Some(fid) if !fid.is_empty() && fid != "best" && fid != "auto" => {
                if fid.contains('+') || fid.to_lowercase().contains("audio") {
                    fid.to_string()
                } else {
                    format!("{}+bestaudio[ext=m4a]/{}+bestaudio/best", fid, fid)
                }
            }
            _ => "bestvideo[ext=mp4]+bestaudio[ext=m4a]/bestvideo+bestaudio/best[ext=mp4]/best".to_string(),
        };
        let output_template = dest_dir.join("%(title)s.%(ext)s").to_string_lossy().to_string();

        let mut cmd = tokio::process::Command::new(bin);
        cmd.arg("-f")
            .arg(&format_arg_str)
            .arg("-o")
            .arg(&output_template)
            .arg("--no-playlist")
            .arg("--newline")
            .arg("--force-overwrites")
            .arg("--merge-output-format")
            .arg("mp4")
            .arg("--js-runtimes")
            .arg("node");

        if let Some(ffmpeg_bin) = Self::find_ffmpeg_binary() {
            cmd.arg("--ffmpeg-location").arg(ffmpeg_bin.to_string_lossy().to_string());
            if let Some(parent_dir) = ffmpeg_bin.parent() {
                if let Ok(current_path) = std::env::var("PATH") {
                    cmd.env("PATH", format!("{};{}", parent_dir.to_string_lossy(), current_path));
                }
            }
        }

        cmd.arg(url);

        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());

        #[cfg(target_os = "windows")]
        {
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn extractor: {}", e))?;
        let stdout = child.stdout.take().ok_or_else(|| "Failed to open stdout".to_string())?;
        let mut reader = tokio::io::BufReader::new(stdout).lines();

        let mut is_audio_pass = false;
        let mut final_path: Option<std::path::PathBuf> = None;
        let mut total_bytes = 0u64;
        let mut video_bytes = 0u64;
        let mut audio_bytes = 0u64;

        while let Ok(Some(line)) = reader.next_line().await {
            if *cancel_rx.borrow() {
                let _ = child.kill().await;
                return Err("Download cancelled".into());
            }

            let raw_line = line.trim();

            // Detect FFmpeg format merger
            if raw_line.contains("[Merger]") || raw_line.contains("Merging formats into") {
                if let Some(start) = raw_line.find('"') {
                    if let Some(end) = raw_line.rfind('"') {
                        if end > start {
                            let merged = &raw_line[start + 1..end];
                            final_path = Some(std::path::PathBuf::from(merged));
                        }
                    }
                }
                let _ = progress_tx.send((0.98, 0.0, total_bytes, total_bytes, 1.0, false, None)).await;
                continue;
            }

            // Detect stream destination
            if raw_line.contains("Destination:") {
                let dest = raw_line["Destination:".len()..].trim();
                let p = std::path::PathBuf::from(dest);
                if let Some(ext) = p.extension().and_then(|e| e.to_str()) {
                    let ext_low = ext.to_lowercase();
                    if ext_low == "m4a" || ext_low == "webm" || ext_low == "opus" || ext_low == "mp3" || ext_low == "aac" {
                        is_audio_pass = true;
                    }
                }
                if final_path.is_none() {
                    final_path = Some(p);
                }
                continue;
            }

            let sub_lines: Vec<&str> = if raw_line.contains("[download]") {
                raw_line.split("[download]").collect()
            } else {
                vec![raw_line]
            };

            for sub in sub_lines {
                let l = sub.trim();
                if l.is_empty() {
                    continue;
                }

                if l.contains('%') {
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
                            if let Some(tb) = Self::parse_bytes_str(tokens[i + 1]) {
                                if is_audio_pass {
                                    audio_bytes = tb;
                                } else {
                                    video_bytes = tb;
                                }
                                total_bytes = video_bytes + audio_bytes;
                            }
                        }
                        if *tok == "at" && i + 1 < tokens.len() {
                            speed = Self::parse_speed_str(tokens[i + 1]);
                        }
                        if *tok == "ETA" && i + 1 < tokens.len() {
                            eta = Self::parse_eta_str(tokens[i + 1]);
                        }
                    }

                    let normalized_progress = if is_audio_pass {
                        0.75 + (raw_percent.min(1.0) * 0.20) // 75% -> 95%
                    } else {
                        raw_percent.min(1.0) * 0.75 // 0% -> 75%
                    };

                    let current_dld = if is_audio_pass {
                        video_bytes + (audio_bytes as f64 * raw_percent.min(1.0)) as u64
                    } else {
                        (video_bytes as f64 * raw_percent.min(1.0)) as u64
                    };

                    let _ = progress_tx.send((normalized_progress, speed, current_dld, total_bytes, eta, false, None)).await;
                }
            }
        }

        let status = child.wait().await.map_err(|e| format!("Child wait failed: {}", e))?;
        if status.success() {
            // Auto-recovery: If split .f*.mp4 and .f*.m4a files exist, merge them directly with FFmpeg
            if let Some(merged_file) = Self::auto_merge_unmerged_parts(dest_dir).await {
                final_path = Some(merged_file);
            }

            let final_pb = final_path.unwrap_or_else(|| dest_dir.to_path_buf());
            let fname = final_pb.file_name().map(|n| n.to_string_lossy().to_string());
            let _ = progress_tx.send((1.0, 0.0, total_bytes, total_bytes, 0.0, true, fname)).await;
            Ok(final_pb)
        } else {
            Err("Video extraction download failed".into())
        }
    }

    /// Automatically find matching video and audio part files in dest_dir and merge them with FFmpeg.
    pub async fn auto_merge_unmerged_parts(dest_dir: &std::path::Path) -> Option<std::path::PathBuf> {
        let mut video_parts = Vec::new();
        let mut audio_parts = Vec::new();

        if let Ok(mut entries) = tokio::fs::read_dir(dest_dir).await {
            while let Ok(Some(entry)) = entries.next_entry().await {
                let path = entry.path();
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if name.contains(".f") && name.ends_with(".mp4") {
                        video_parts.push(path);
                    } else if name.contains(".f") && (name.ends_with(".m4a") || name.ends_with(".webm") || name.ends_with(".opus")) {
                        audio_parts.push(path);
                    }
                }
            }
        }

        if let Some(ffmpeg_bin) = Self::find_ffmpeg_binary() {
            for vpath in &video_parts {
                let vname = vpath.file_name()?.to_str()?;
                let base_name = if let Some(idx) = vname.rfind(".f") {
                    &vname[..idx]
                } else {
                    continue;
                };

                if let Some(apath) = audio_parts.iter().find(|ap| {
                    ap.file_name().and_then(|n| n.to_str()).map(|an| an.starts_with(base_name)).unwrap_or(false)
                }) {
                    let final_merged = dest_dir.join(format!("{}.mp4", base_name));
                    let mut cmd = Command::new(&ffmpeg_bin);
                    cmd.arg("-y")
                        .arg("-i").arg(vpath)
                        .arg("-i").arg(apath)
                        .arg("-c").arg("copy")
                        .arg(&final_merged);

                    #[cfg(target_os = "windows")]
                    {
                        use std::os::windows::process::CommandExt;
                        cmd.creation_flags(0x08000000);
                    }

                    if let Ok(status) = cmd.status() {
                        if status.success() {
                            let _ = tokio::fs::remove_file(vpath).await;
                            let _ = tokio::fs::remove_file(apath).await;
                            return Some(final_merged);
                        }
                    }
                }
            }
        }
        None
    }

    fn parse_bytes_str(s: &str) -> Option<u64> {
        let clean = s.trim().to_lowercase();
        let num_str = clean.trim_end_matches(|c: char| !c.is_numeric() && c != '.');
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
        let clean = s.trim().to_lowercase();
        let num_str = clean.trim_end_matches(|c: char| !c.is_numeric() && c != '.');
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
        let clean_url = url.trim();

        // 1. Cinemana Shabakaty special extractor
        if clean_url.contains("cinemana.shabakaty.com") {
            if let Ok(info) = Self::extract_cinemana_info(client, clean_url).await {
                return Ok(info);
            }
        }

        // 2. Try yt-dlp if available (supports 1000+ complex platforms including YouTube, TikTok, Vimeo)
        match Self::extract_with_ytdlp(clean_url) {
            Ok(info) => return Ok(info),
            Err(e) => {
                // If it's explicitly a video platform, return the actual extraction error
                if clean_url.contains("youtube.com") || clean_url.contains("youtu.be") || clean_url.contains("tiktok.com") {
                    return Err(e);
                }
            }
        }

        // 3. Native HTML & Media Sniffer Fallback
        Self::extract_with_native_sniffer(client, clean_url).await
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
        let bin = Self::find_ytdlp_binary().ok_or_else(|| "yt-dlp binary not found on system".to_string())?;

        let mut cmd = Command::new(bin);
        cmd.arg("-J")
            .arg("--no-playlist")
            .arg("--no-warnings")
            .arg("--user-agent")
            .arg(DEFAULT_USER_AGENT)
            .arg(url);

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
            let first_line = err_msg.lines().find(|l| l.contains("ERROR:")).unwrap_or("Video unavailable or invalid URL");
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
