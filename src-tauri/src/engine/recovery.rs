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

    /// Calculate adaptive backoff with randomized jitter.
    pub fn calculate_backoff(retry_count: usize, is_rate_limited: bool) -> Duration {
        let multiplier = if is_rate_limited { 4 } else { 2 };
        let base_ms = RETRY_BASE_DELAY_MS * (multiplier as u64).pow(retry_count.min(6) as u32);
        let jitter = (rand::random::<u64>() % (base_ms / 2 + 1)) as i64 - (base_ms as i64 / 4);
        let delay_ms = (base_ms as i64 + jitter).max(200) as u64;
        Duration::from_millis(delay_ms)
    }

    /// Re-extract a fresh direct download URL if the original link expired.
    pub async fn refresh_expired_link(
        client: &Client,
        original_page_url: &str,
        meta: &mut DownloadMetadata,
    ) -> Result<String, String> {
        let extraction = MediaExtractor::extract_info(client, original_page_url).await?;

        // Find best matching format or primary URL
        let fresh_url = if let Some(fmt) = extraction.formats.first() {
            fmt.url.clone().unwrap_or_else(|| extraction.url.clone())
        } else {
            extraction.url
        };

        if fresh_url.is_empty() || fresh_url == meta.url {
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
