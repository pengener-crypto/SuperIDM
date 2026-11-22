use std::time::Duration;
use reqwest::Client;

use super::extractor::MediaExtractor;
use super::types::{DownloadMetadata, RETRY_BASE_DELAY_MS};

/// Recovery and link refresh engine for expired tokens and network interruptions.
pub struct RecoveryEngine;

impl RecoveryEngine {
    /// Check if an HTTP status indicates an expired temporary link or token.
    pub fn is_token_expired(status_code: u16) -> bool {
        status_code == 403 || status_code == 410 || status_code == 401
    }

    /// Calculate adaptive backoff with randomized jitter (capped at 30 seconds).
    pub fn calculate_backoff(retry_count: usize, is_rate_limited: bool) -> Duration {
        let multiplier: u64 = if is_rate_limited { 4 } else { 2 };
        let pow = (retry_count.min(6)) as u32;
        let base_ms = RETRY_BASE_DELAY_MS
            .saturating_mul(multiplier.saturating_pow(pow))
            .min(30_000);
        let jitter = (rand::random::<u64>() % (base_ms / 2 + 1)) as i64 - (base_ms as i64 / 4);
        let delay_ms = (base_ms as i64 + jitter).clamp(200, 30_000) as u64;
        Duration::from_millis(delay_ms)
    }

    /// Re-extract a fresh direct download URL if the original link expired.
    pub async fn refresh_expired_link(
        client: &Client,
        original_page_url: &str,
        meta: &mut DownloadMetadata,
    ) -> Result<String, String> {
        let clean_source = MediaExtractor::clean_target_url(original_page_url);
        let extraction = MediaExtractor::extract_info(client, &clean_source).await?;

        // For known streaming platforms (YouTube, TikTok, etc.), keep the canonical page URL so
        // yt-dlp can continue merging high-res video+audio instead of switching to a single raw track.
        if MediaExtractor::is_platform_url(&clean_source) {
            meta.url = clean_source.clone();
            meta.updated_at = chrono::Utc::now().timestamp_millis();
            return Ok(clean_source);
        }

        // Match saved format_id if present, otherwise pick the highest-quality format (last/largest),
        // falling back to extraction.url.
        let fresh_url = meta
            .format_id
            .as_deref()
            .and_then(|wanted| {
                extraction
                    .formats
                    .iter()
                    .find(|f| f.format_id == wanted && f.url.as_deref().map(|u| !u.is_empty()).unwrap_or(false))
                    .and_then(|f| f.url.clone())
            })
            .or_else(|| {
                extraction
                    .formats
                    .iter()
                    .rev()
                    .find_map(|f| f.url.clone().filter(|u| u.starts_with("http://") || u.starts_with("https://")))
            })
            .unwrap_or_else(|| extraction.url.clone());

        if fresh_url.is_empty() {
            return Err("Failed to obtain fresh link".into());
        }

        meta.url = fresh_url.clone();
        meta.updated_at = chrono::Utc::now().timestamp_millis();

        Ok(fresh_url)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_token_expired() {
        assert!(RecoveryEngine::is_token_expired(403));
        assert!(RecoveryEngine::is_token_expired(410));
        assert!(RecoveryEngine::is_token_expired(401));
        assert!(!RecoveryEngine::is_token_expired(500));
        assert!(!RecoveryEngine::is_token_expired(200));
    }

    #[test]
    fn test_backoff_calculation() {
        let d1 = RecoveryEngine::calculate_backoff(1, false);
        let d2 = RecoveryEngine::calculate_backoff(2, false);
        assert!(d2.as_millis() >= d1.as_millis());

        let rate_limited = RecoveryEngine::calculate_backoff(1, true);
        assert!(rate_limited.as_millis() >= d1.as_millis());
    }
}
