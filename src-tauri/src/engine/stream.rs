use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use reqwest::Client;
use tokio::sync::{mpsc, watch};

use super::session::DEFAULT_USER_AGENT;
use super::types::StreamSegment;

/// HLS & DASH Streaming Engine.
pub struct StreamEngine;

impl StreamEngine {
    /// Parse an HLS .m3u8 playlist manifest and extract all segments.
    pub async fn parse_hls_manifest(client: &Client, manifest_url: &str) -> Result<Vec<StreamSegment>, String> {
        let resp = client
            .get(manifest_url)
            .header("User-Agent", DEFAULT_USER_AGENT)
            .header("Accept", "*/*")
            .send()
            .await
            .map_err(|e| format!("Failed to fetch HLS playlist: {}", e))?;

        let body = resp.text().await.map_err(|e| format!("Failed to read HLS playlist: {}", e))?;
        let base_url = reqwest::Url::parse(manifest_url).map_err(|e| format!("Invalid manifest URL: {}", e))?;

        // Check if master playlist (contains #EXT-X-STREAM-INF)
        if body.contains("#EXT-X-STREAM-INF") {
            // Find highest bandwidth sub-playlist
            let mut highest_uri = None;
            let mut highest_bandwidth = 0u64;

            let lines: Vec<&str> = body.lines().collect();
            for i in 0..lines.len() {
                let line = lines[i].trim();
                if line.starts_with("#EXT-X-STREAM-INF:") {
                    let mut bw = 0u64;
                    if let Some(pos) = line.find("BANDWIDTH=") {
                        let rest = &line[pos + 10..];
                        let val_str = rest.split(',').next().unwrap_or("0");
                        bw = val_str.parse().unwrap_or(0);
                    }
                    if i + 1 < lines.len() {
                        let next_line = lines[i + 1].trim();
                        if !next_line.starts_with('#') && !next_line.is_empty() {
                            if bw >= highest_bandwidth || highest_uri.is_none() {
                                highest_bandwidth = bw;
                                highest_uri = Some(next_line);
                            }
                        }
                    }
                }
            }

            if let Some(sub_uri) = highest_uri {
                let resolved_url = base_url.join(sub_uri).map_err(|e| format!("Failed to resolve sub-playlist URL: {}", e))?;
                return Box::pin(Self::parse_hls_manifest(client, resolved_url.as_str())).await;
            }
        }

        // Media Playlist parsing
        let mut segments = Vec::new();
        let mut cur_duration = 0.0f64;
        let mut index = 0;

        for line in body.lines() {
            let l = line.trim();
            if l.starts_with("#EXTINF:") {
                let dur_str = l.trim_start_matches("#EXTINF:").split(',').next().unwrap_or("0");
                cur_duration = dur_str.parse().unwrap_or(0.0);
            } else if !l.starts_with('#') && !l.is_empty() {
                let seg_url = base_url
                    .join(l)
                    .map_err(|e| format!("Failed to resolve segment URL '{}': {}", l, e))?
                    .to_string();

                segments.push(StreamSegment {
                    index,
                    url: seg_url,
                    duration_secs: cur_duration,
                    byte_range: None,
                    completed: false,
                });
                index += 1;
            }
        }

        if segments.is_empty() {
            return Err("No segments found in HLS playlist".into());
        }

        Ok(segments)
    }

    /// Download all HLS segments concurrently and stitch them into a final video file.
    pub async fn download_hls_stream(
        client: Client,
        manifest_url: String,
        output_path: PathBuf,
        concurrency: usize,
        progress_tx: mpsc::Sender<(usize, usize, u64)>, // (completed_segments, total_segments, total_bytes)
        cancel_rx: watch::Receiver<bool>,
    ) -> Result<(), String> {
        let segments = Self::parse_hls_manifest(&client, &manifest_url).await?;
        let total_segs = segments.len();

        let temp_dir = tempfile::tempdir().map_err(|e| format!("Failed to create tempdir: {}", e))?;
        let temp_path = temp_dir.path().to_path_buf();

        let completed_count = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let total_bytes_downloaded = Arc::new(AtomicU64::new(0));

        let semaphore = Arc::new(tokio::sync::Semaphore::new(concurrency.clamp(2, 16)));
        let mut tasks = Vec::new();

        for seg in segments {
            let sem = semaphore.clone();
            let c_client = client.clone();
            let c_cancel = cancel_rx.clone();
            let c_comp = completed_count.clone();
            let c_bytes = total_bytes_downloaded.clone();
            let p_tx = progress_tx.clone();
            let seg_file = temp_path.join(format!("seg_{:05}.ts", seg.index));

            let task = tokio::spawn(async move {
                let _permit = sem.acquire().await.map_err(|e| format!("Semaphore error: {}", e))?;
                if *c_cancel.borrow() {
                    return Ok(());
                }

                let resp = c_client
                    .get(&seg.url)
                    .header("User-Agent", DEFAULT_USER_AGENT)
                    .header("Accept", "*/*")
                    .send()
                    .await
                    .map_err(|e| format!("Segment {} download failed: {}", seg.index, e))?;

                let bytes = resp.bytes().await.map_err(|e| format!("Segment {} read failed: {}", seg.index, e))?;
                let byte_len = bytes.len() as u64;

                tokio::fs::write(&seg_file, &bytes)
                    .await
                    .map_err(|e| format!("Segment {} write failed: {}", seg.index, e))?;

                let done = c_comp.fetch_add(1, Ordering::SeqCst) + 1;
                let tot_b = c_bytes.fetch_add(byte_len, Ordering::SeqCst) + byte_len;
                let _ = p_tx.send((done, total_segs, tot_b)).await;

                Ok::<(), String>(())
            });

            tasks.push(task);
        }

        for t in tasks {
            let res = t.await.map_err(|e| format!("Join error: {}", e))?;
            res?;
        }

        // Stitch segments into output file
        Self::stitch_segments(&temp_path, total_segs, &output_path).await?;

        Ok(())
    }

    /// Stitch downloaded .ts segments in numerical order into the final file.
    async fn stitch_segments(temp_dir: &Path, total_segments: usize, output_path: &Path) -> Result<(), String> {
        // Try FFmpeg concatenation first for clean lossless container muxing
        if Self::try_ffmpeg_mux(temp_dir, total_segments, output_path).is_ok() {
            return Ok(());
        }

        // Direct byte-stream TS concatenation fallback
        use tokio::io::AsyncWriteExt;
        let mut out_file = tokio::fs::File::create(output_path)
            .await
            .map_err(|e| format!("Failed to create output file: {}", e))?;

        for i in 0..total_segments {
            let seg_path = temp_dir.join(format!("seg_{:05}.ts", i));
            if let Ok(data) = tokio::fs::read(&seg_path).await {
                out_file
                    .write_all(&data)
                    .await
                    .map_err(|e| format!("Failed to write stitched segment {}: {}", i, e))?;
            }
        }

        out_file.flush().await.map_err(|e| format!("Flush error: {}", e))?;
        Ok(())
    }

    /// Try lossless FFmpeg remuxing into standard MP4.
    fn try_ffmpeg_mux(temp_dir: &Path, total_segments: usize, output_path: &Path) -> Result<(), String> {
        let list_file = temp_dir.join("concat_list.txt");
        let mut content = String::new();
        for i in 0..total_segments {
            let seg_name = format!("seg_{:05}.ts", i);
            content.push_str(&format!("file '{}'\n", seg_name));
        }

        std::fs::write(&list_file, &content).map_err(|e| format!("Failed to write concat list: {}", e))?;

        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-f")
            .arg("concat")
            .arg("-safe")
            .arg("0")
            .arg("-i")
            .arg(&list_file)
            .arg("-c")
            .arg("copy")
            .arg("-y")
            .arg(output_path);

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000); // CREATE_NO_WINDOW
        }

        let status = cmd.status().map_err(|e| format!("FFmpeg error: {}", e))?;
        if status.success() {
            Ok(())
        } else {
            Err("FFmpeg muxing failed".into())
        }
    }
}
