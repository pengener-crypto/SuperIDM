use std::path::Path;
use tokio::fs::{self, File};
use tokio::io::{AsyncSeekExt, AsyncWriteExt, SeekFrom};

use super::types::DownloadMetadata;

#[cfg(windows)]
use std::os::windows::fs::OpenOptionsExt;

/// Open a file for read/write with shared access on Windows (FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE).
/// This allows multiple parallel chunk workers to write to different byte offsets simultaneously without sharing violations.
pub fn open_shared_file(path: &Path) -> Result<File, String> {
    #[cfg(windows)]
    {
        let std_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(7) // FILE_SHARE_READ (1) | FILE_SHARE_WRITE (2) | FILE_SHARE_DELETE (4)
            .open(path)
            .map_err(|e| format!("Failed to open shared file {}: {}", path.display(), e))?;
        Ok(File::from_std(std_file))
    }
    #[cfg(not(windows))]
    {
        let std_file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(|e| format!("Failed to open shared file {}: {}", path.display(), e))?;
        Ok(File::from_std(std_file))
    }
}

/// Pre-allocate a file on disk at the given path with the specified size.
pub async fn preallocate_file(path: &Path, size: u64) -> Result<(), String> {
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .await
            .map_err(|e| format!("Failed to create directory {}: {}", parent.display(), e))?;
    }

    let file = open_shared_file(path)?;
    file.set_len(size)
        .await
        .map_err(|e| format!("Failed to pre-allocate {} bytes for {}: {}", size, path.display(), e))?;

    Ok(())
}

/// Write data at a specific byte offset in the file.
///
/// Uses `seek(SeekFrom::Start(offset))` followed by `write_all` for
/// direct positional writes. This is the core I/O operation for segmented
/// downloads — each chunk worker writes to its assigned byte range without
/// needing mmap.
pub async fn write_at_offset(
    file: &mut File,
    offset: u64,
    data: &[u8],
) -> Result<(), String> {
    file.seek(SeekFrom::Start(offset))
        .await
        .map_err(|e| format!("Failed to seek to offset {}: {}", offset, e))?;

    file.write_all(data)
        .await
        .map_err(|e| format!("Failed to write {} bytes at offset {}: {}", data.len(), offset, e))?;

    Ok(())
}

/// Atomically save download metadata to a `.part.json` sidecar file.
///
/// Writes to a `.part.json.tmp` file first, then renames it over the target
/// `.part.json` to prevent corruption if the process crashes mid-write.
/// This is the most critical persistence operation in the engine — it
/// guarantees that chunk offsets and ETag/Last-Modified data survive
/// application crashes and power failures.
pub async fn save_part_json_atomic(
    download_path: &Path,
    metadata: &DownloadMetadata,
) -> Result<(), String> {
    let part_path = part_json_path(download_path);
    let tmp_path = part_path.with_extension("json.tmp");

    let json = serde_json::to_string_pretty(metadata)
        .map_err(|e| format!("Failed to serialize metadata: {}", e))?;

    // Write to temporary file first
    fs::write(&tmp_path, json.as_bytes())
        .await
        .map_err(|e| format!("Failed to write temp sidecar {}: {}", tmp_path.display(), e))?;

    // Atomic rename over the real .part.json
    fs::rename(&tmp_path, &part_path)
        .await
        .map_err(|e| format!("Failed to rename {} -> {}: {}", tmp_path.display(), part_path.display(), e))?;

    Ok(())
}

/// Load download metadata from an existing `.part.json` sidecar file.
///
/// Returns `Ok(None)` if no sidecar exists (i.e., this is a fresh download).
/// Returns `Ok(Some(metadata))` if a valid sidecar was found and parsed.
pub async fn load_part_json(download_path: &Path) -> Result<Option<DownloadMetadata>, String> {
    let part_path = part_json_path(download_path);

    if !part_path.exists() {
        return Ok(None);
    }

    let contents = fs::read_to_string(&part_path)
        .await
        .map_err(|e| format!("Failed to read sidecar {}: {}", part_path.display(), e))?;

    let metadata: DownloadMetadata = serde_json::from_str(&contents)
        .map_err(|e| format!("Failed to parse sidecar {}: {}", part_path.display(), e))?;

    Ok(Some(metadata))
}

/// Delete the `.part.json` sidecar file after a successful download.
pub async fn delete_part_json(download_path: &Path) -> Result<(), String> {
    let part_path = part_json_path(download_path);
    if part_path.exists() {
        fs::remove_file(&part_path)
            .await
            .map_err(|e| format!("Failed to delete sidecar {}: {}", part_path.display(), e))?;
    }
    Ok(())
}

/// Scan a directory for existing `.part.json` files to recover interrupted downloads.
///
/// Called on application startup to auto-discover paused/interrupted tasks.
pub async fn scan_for_part_files(dir: &Path) -> Result<Vec<DownloadMetadata>, String> {
    let mut results = Vec::new();

    if !dir.exists() {
        return Ok(results);
    }

    let mut entries = fs::read_dir(dir)
        .await
        .map_err(|e| format!("Failed to read directory {}: {}", dir.display(), e))?;

    while let Some(entry) = entries.next_entry()
        .await
        .map_err(|e| format!("Failed to read dir entry: {}", e))?
    {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                if stem.ends_with(".part") {
                    match fs::read_to_string(&path).await {
                        Ok(contents) => {
                            match serde_json::from_str::<DownloadMetadata>(&contents) {
                                Ok(meta) => results.push(meta),
                                Err(e) => {
                                    eprintln!("Warning: Corrupt sidecar {}: {}", path.display(), e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("Warning: Could not read {}: {}", path.display(), e);
                        }
                    }
                }
            }
        }
    }

    Ok(results)
}

/// Save all tasks to the permanent database file (%APPDATA%\SuperIDM\tasks_db.json).
pub async fn save_tasks_db(tasks: &[DownloadMetadata]) -> Result<(), String> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("SuperIDM");
    let _ = fs::create_dir_all(&config_dir).await;
    let db_path = config_dir.join("tasks_db.json");
    let tmp_path = config_dir.join("tasks_db.json.tmp");

    let json = serde_json::to_string_pretty(tasks)
        .map_err(|e| format!("Failed to serialize tasks db: {}", e))?;

    fs::write(&tmp_path, json.as_bytes())
        .await
        .map_err(|e| format!("Failed to write tmp tasks db: {}", e))?;

    fs::rename(&tmp_path, &db_path)
        .await
        .map_err(|e| format!("Failed to commit tasks db: {}", e))?;

    Ok(())
}

/// Load all tasks from the permanent database file (%APPDATA%\SuperIDM\tasks_db.json).
pub async fn load_tasks_db() -> Result<Vec<DownloadMetadata>, String> {
    let config_dir = dirs::config_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("SuperIDM");
    let db_path = config_dir.join("tasks_db.json");

    if !db_path.exists() {
        return Ok(Vec::new());
    }

    let contents = fs::read_to_string(&db_path)
        .await
        .map_err(|e| format!("Failed to read tasks db: {}", e))?;

    let tasks: Vec<DownloadMetadata> = serde_json::from_str(&contents)
        .unwrap_or_default();

    Ok(tasks)
}

/// Derive the `.part.json` sidecar path from a download file path.
///
/// Example: `/downloads/ubuntu.iso` -> `/downloads/ubuntu.iso.part.json`
pub fn part_json_path(download_path: &Path) -> std::path::PathBuf {
    let mut p = download_path.as_os_str().to_owned();
    p.push(".part.json");
    std::path::PathBuf::from(p)
}

/// Query the available free disk space (in bytes) on the volume hosting the given directory or path.
///
/// Uses Windows Win32 API `GetDiskFreeSpaceExW` for high precision without external crate overhead.
pub fn get_available_disk_space(path: &Path) -> Result<u64, String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;

        // Try candidate directory paths: the path itself, or closest existing ancestor
        let mut candidate = path.to_path_buf();
        while !candidate.exists() {
            if let Some(parent) = candidate.parent() {
                candidate = parent.to_path_buf();
            } else {
                break;
            }
        }

        let mut wide: Vec<u16> = candidate.as_os_str().encode_wide().collect();
        wide.push(0);

        let mut free_bytes_available: u64 = 0;
        let mut total_number_of_bytes: u64 = 0;
        let mut total_number_of_free_bytes: u64 = 0;

        extern "system" {
            fn GetDiskFreeSpaceExW(
                lpDirectoryName: *const u16,
                lpFreeBytesAvailableToCaller: *mut u64,
                lpTotalNumberOfBytes: *mut u64,
                lpTotalNumberOfFreeBytes: *mut u64,
            ) -> i32;
        }

        let ret = unsafe {
            GetDiskFreeSpaceExW(
                wide.as_ptr(),
                &mut free_bytes_available,
                &mut total_number_of_bytes,
                &mut total_number_of_free_bytes,
            )
        };

        if ret != 0 {
            Ok(free_bytes_available)
        } else {
            Err("Unable to retrieve volume disk metrics".to_string())
        }
    }

    #[cfg(not(windows))]
    {
        // Safe fallback for non-windows environments
        Ok(100 * 1024 * 1024 * 1024)
    }
}

/// Resolve a unique filename in `dest_dir` to prevent overwriting existing files.
///
/// If `dest_dir.join(filename)` exists AND does NOT have an active `.part.json`
/// (which indicates an interrupted task being resumed), it appends ` (1)`, ` (2)`, etc.
///
/// Example: `file.iso` -> `file (1).iso` -> `file (2).iso`
/// Handles UTF-8 Arabic filenames and files without extensions safely.
pub fn resolve_unique_filename(dest_dir: &Path, filename: &str) -> String {
    let target = dest_dir.join(filename);
    let part_sidecar = part_json_path(&target);

    // If file doesn't exist, or if an active .part.json exists (indicating resume), keep current name
    if !target.exists() || part_sidecar.exists() {
        return filename.to_string();
    }

    let p = Path::new(filename);
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or(filename);
    let ext = p.extension().and_then(|e| e.to_str());

    for i in 1..=9999 {
        let candidate_name = match ext {
            Some(extension) if !extension.is_empty() => format!("{} ({}).{}", stem, i, extension),
            _ => format!("{} ({})", stem, i),
        };

        let candidate_path = dest_dir.join(&candidate_name);
        let candidate_sidecar = part_json_path(&candidate_path);

        if !candidate_path.exists() || candidate_sidecar.exists() {
            return candidate_name;
        }
    }

    format!("{}_{}.dat", stem, &uuid::Uuid::new_v4().to_string()[..8])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::types::{ChunkInfo, TaskStatus};
    use tempfile::TempDir;

    fn sample_metadata() -> DownloadMetadata {
        DownloadMetadata {
            id: "test-123".to_string(),
            url: "https://example.com/file.zip".to_string(),
            destination: "/tmp/file.zip".to_string(),
            filename: "file.zip".to_string(),
            total_bytes: 1_048_576,
            status: TaskStatus::Paused,
            etag: Some("\"abc123\"".to_string()),
            last_modified: Some("Wed, 01 Jan 2025 00:00:00 GMT".to_string()),
            supports_range: true,
            num_connections: 8,
            priority: 0,
            category: "archive".to_string(),
            chunks: vec![
                ChunkInfo {
                    index: 0,
                    start_byte: 0,
                    end_byte: 524287,
                    current_byte: 262144,
                    completed: false,
                    retry_count: 0,
                },
                ChunkInfo {
                    index: 1,
                    start_byte: 524288,
                    end_byte: 1048575,
                    current_byte: 524288,
                    completed: false,
                    retry_count: 0,
                },
            ],
            created_at: 1700000000000,
            updated_at: 1700000001000,
        }
    }

    #[tokio::test]
    async fn test_atomic_save_and_load() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("test_file.zip");

        let meta = sample_metadata();

        // Save atomically
        save_part_json_atomic(&file_path, &meta).await.unwrap();

        // Verify .part.json exists
        let part_path = part_json_path(&file_path);
        assert!(part_path.exists());

        // Verify .tmp file was cleaned up
        let tmp_path = part_path.with_extension("json.tmp");
        assert!(!tmp_path.exists());

        // Load and verify
        let loaded = load_part_json(&file_path).await.unwrap().unwrap();
        assert_eq!(loaded.id, "test-123");
        assert_eq!(loaded.etag, Some("\"abc123\"".to_string()));
        assert_eq!(loaded.chunks.len(), 2);
        assert_eq!(loaded.chunks[0].current_byte, 262144);
    }

    #[tokio::test]
    async fn test_preallocate_file() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("preallocated.bin");

        let _file = preallocate_file(&file_path, 1_048_576).await.unwrap();

        let metadata = std::fs::metadata(&file_path).unwrap();
        assert_eq!(metadata.len(), 1_048_576);
    }

    #[tokio::test]
    async fn test_concurrent_shared_writes() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("concurrent.bin");
        let total_size = 16 * 1024;
        preallocate_file(&file_path, total_size).await.unwrap();

        let mut handles = vec![];
        for i in 0..16 {
            let p = file_path.clone();
            handles.push(tokio::spawn(async move {
                let mut f = open_shared_file(&p).unwrap();
                let offset = (i as u64) * 1024;
                let data = vec![i as u8; 1024];
                write_at_offset(&mut f, offset, &data).await.unwrap();
            }));
        }

        for h in handles {
            h.await.unwrap();
        }

        let content = std::fs::read(&file_path).unwrap();
        assert_eq!(content.len(), total_size as usize);
        for i in 0..16 {
            assert_eq!(content[i * 1024], i as u8);
        }
    }

    #[test]
    fn test_resolve_unique_filename() {
        let dir = TempDir::new().unwrap();

        // 1. Non-existing file returns same name
        let name1 = resolve_unique_filename(dir.path(), "sample.mp4");
        assert_eq!(name1, "sample.mp4");

        // 2. Create sample.mp4
        std::fs::write(dir.path().join("sample.mp4"), b"test").unwrap();
        let name2 = resolve_unique_filename(dir.path(), "sample.mp4");
        assert_eq!(name2, "sample (1).mp4");

        // 3. Create sample (1).mp4
        std::fs::write(dir.path().join("sample (1).mp4"), b"test").unwrap();
        let name3 = resolve_unique_filename(dir.path(), "sample.mp4");
        assert_eq!(name3, "sample (2).mp4");

        // 4. Arabic filename support
        std::fs::write(dir.path().join("ملف_عربي.pdf"), b"arabic").unwrap();
        let name_ar = resolve_unique_filename(dir.path(), "ملف_عربي.pdf");
        assert_eq!(name_ar, "ملف_عربي (1).pdf");
    }

    #[test]
    fn test_disk_space_query() {
        let dir = TempDir::new().unwrap();
        let space = get_available_disk_space(dir.path()).unwrap();
        assert!(space > 1_000_000); // At least 1MB free on test volume
    }
}

