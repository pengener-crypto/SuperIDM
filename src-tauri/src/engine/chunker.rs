use super::types::ChunkInfo;

/// Divide a total byte range into N even segments for parallel downloading.
///
/// Each chunk gets a contiguous byte range [start_byte, end_byte].
/// If the server doesn't support Range headers, returns a single chunk
/// covering the full file.
pub fn calculate_chunks(total_bytes: u64, num_connections: usize) -> Vec<ChunkInfo> {
    if total_bytes == 0 {
        return vec![];
    }

    let n = num_connections.max(1) as u64;
    let chunk_size = total_bytes / n;
    let remainder = total_bytes % n;

    let mut chunks = Vec::with_capacity(num_connections);
    let mut offset = 0u64;

    for i in 0..n {
        // Distribute remainder bytes across the first `remainder` chunks
        let extra = if i < remainder { 1 } else { 0 };
        let size = chunk_size + extra;
        let end = offset + size - 1;

        chunks.push(ChunkInfo {
            index: i as usize,
            start_byte: offset,
            end_byte: end,
            current_byte: offset,
            completed: false,
            retry_count: 0,
        });

        offset = end + 1;
    }

    chunks
}

/// Dynamic work-stealing: when a worker finishes its chunk early, it finds
/// the chunk with the most remaining bytes and splits its unfinished portion.
///
/// Returns `Some((donor_index, new_chunk))` if a split was performed,
/// or `None` if no chunk has enough remaining bytes to split (minimum 64KB).
pub fn steal_work(chunks: &mut [ChunkInfo]) -> Option<(usize, ChunkInfo)> {
    const MIN_STEAL_BYTES: u64 = 65_536; // 64 KB minimum to justify a new connection

    // Find the chunk with the most remaining bytes
    let (donor_idx, donor_remaining) = chunks
        .iter()
        .enumerate()
        .filter(|(_, c)| !c.completed)
        .max_by_key(|(_, c)| c.remaining())?;

    if donor_remaining.remaining() < MIN_STEAL_BYTES * 2 {
        return None; // Not enough to split
    }

    let donor = &chunks[donor_idx];
    let midpoint = donor.current_byte + donor.remaining() / 2;

    // Create new chunk for the second half
    let new_chunk = ChunkInfo {
        index: chunks.len(), // Will be appended
        start_byte: midpoint,
        end_byte: donor.end_byte,
        current_byte: midpoint,
        completed: false,
        retry_count: 0,
    };

    // Shrink the donor to cover only the first half
    chunks[donor_idx].end_byte = midpoint - 1;

    Some((donor_idx, new_chunk))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_chunks_basic() {
        let chunks = calculate_chunks(1_000_000, 4);
        assert_eq!(chunks.len(), 4);

        // Verify complete coverage with no gaps
        for i in 0..chunks.len() {
            assert_eq!(chunks[i].index, i);
            assert_eq!(chunks[i].current_byte, chunks[i].start_byte);
            assert!(!chunks[i].completed);
            assert_eq!(chunks[i].retry_count, 0);

            if i > 0 {
                assert_eq!(chunks[i].start_byte, chunks[i - 1].end_byte + 1);
            }
        }

        // First chunk starts at 0, last ends at total - 1
        assert_eq!(chunks[0].start_byte, 0);
        assert_eq!(chunks.last().unwrap().end_byte, 999_999);

        // Total sizes sum to total_bytes
        let total: u64 = chunks.iter().map(|c| c.total_size()).sum();
        assert_eq!(total, 1_000_000);
    }

    #[test]
    fn test_calculate_chunks_single() {
        let chunks = calculate_chunks(1_000_000, 1);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].start_byte, 0);
        assert_eq!(chunks[0].end_byte, 999_999);
    }

    #[test]
    fn test_calculate_chunks_uneven() {
        // 100 bytes into 3 chunks = 34, 33, 33
        let chunks = calculate_chunks(100, 3);
        assert_eq!(chunks.len(), 3);
        assert_eq!(chunks[0].total_size(), 34);
        assert_eq!(chunks[1].total_size(), 33);
        assert_eq!(chunks[2].total_size(), 33);
        assert_eq!(chunks.last().unwrap().end_byte, 99);
    }

    #[test]
    fn test_calculate_chunks_zero() {
        let chunks = calculate_chunks(0, 8);
        assert!(chunks.is_empty());
    }

    #[test]
    fn test_steal_work() {
        let mut chunks = vec![
            ChunkInfo {
                index: 0,
                start_byte: 0,
                end_byte: 499_999,
                current_byte: 499_999,
                completed: true,
                retry_count: 0,
            },
            ChunkInfo {
                index: 1,
                start_byte: 500_000,
                end_byte: 999_999,
                current_byte: 500_000,
                completed: false,
                retry_count: 0,
            },
        ];

        let result = steal_work(&mut chunks);
        assert!(result.is_some());

        let (donor_idx, new_chunk) = result.unwrap();
        assert_eq!(donor_idx, 1);

        // Donor should now cover first half, new chunk covers second half
        assert!(chunks[1].end_byte < 999_999);
        assert_eq!(new_chunk.end_byte, 999_999);
        assert_eq!(new_chunk.start_byte, chunks[1].end_byte + 1);
    }

    #[test]
    fn test_steal_work_too_small() {
        let mut chunks = vec![
            ChunkInfo {
                index: 0,
                start_byte: 0,
                end_byte: 100,
                current_byte: 50,
                completed: false,
                retry_count: 0,
            },
        ];

        // Only 51 bytes remaining — too small to steal
        let result = steal_work(&mut chunks);
        assert!(result.is_none());
    }
}
