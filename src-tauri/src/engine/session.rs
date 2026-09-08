use std::collections::HashMap;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue, AUTHORIZATION, COOKIE, ORIGIN, REFERER, USER_AGENT};

pub const DEFAULT_USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/124.0.0.0 Safari/537.36";

/// Configuration for session headers and cookies.
#[derive(Debug, Clone, Default)]
pub struct SessionConfig {
    pub user_agent: Option<String>,
    pub referer: Option<String>,
    pub origin: Option<String>,
    pub authorization: Option<String>,
    pub cookies: HashMap<String, String>,
    pub custom_headers: Vec<(String, String)>,
}

impl SessionConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Set standard User-Agent or default Chrome UA.
    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    /// Automatically infer Referer and Origin from a URL if not explicitly set.
    pub fn with_inferred_headers(mut self, url: &str) -> Self {
        if let Ok(parsed) = reqwest::Url::parse(url) {
            if self.origin.is_none() {
                if let Some(host) = parsed.host_str() {
                    let scheme = parsed.scheme();
                    self.origin = Some(format!("{}://{}", scheme, host));
                }
            }
            if self.referer.is_none() {
                self.referer = Some(url.to_string());
            }
        }
        self
    }

    /// Add a cookie key-value pair.
    pub fn add_cookie(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.cookies.insert(key.into(), value.into());
    }

    /// Parse a raw Netscape format cookie string or standard Cookie header string.
    pub fn parse_cookie_header(&mut self, raw: &str) {
        for item in raw.split(';') {
            let part = item.trim();
            if let Some((k, v)) = part.split_once('=') {
                self.cookies.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
    }

    /// Build a reqwest `HeaderMap` with full anti-hotlink and session headers.
    pub fn build_headers(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();

        // User-Agent
        let ua = self.user_agent.as_deref().unwrap_or(DEFAULT_USER_AGENT);
        if let Ok(v) = HeaderValue::from_str(ua) {
            headers.insert(USER_AGENT, v);
        }

        // Accept
        headers.insert(reqwest::header::ACCEPT, HeaderValue::from_static("*/*"));

        // Accept-Language
        headers.insert(reqwest::header::ACCEPT_LANGUAGE, HeaderValue::from_static("en-US,en;q=0.9"));

        // Referer
        if let Some(ref r) = self.referer {
            if let Ok(v) = HeaderValue::from_str(r) {
                headers.insert(REFERER, v);
            }
        }

        // Origin
        if let Some(ref o) = self.origin {
            if let Ok(v) = HeaderValue::from_str(o) {
                headers.insert(ORIGIN, v);
            }
        }

        // Authorization
        if let Some(ref a) = self.authorization {
            if let Ok(v) = HeaderValue::from_str(a) {
                headers.insert(AUTHORIZATION, v);
            }
        }

        // Cookies
        if !self.cookies.is_empty() {
            let cookie_str = self
                .cookies
                .iter()
                .map(|(k, v)| format!("{}={}", k, v))
                .collect::<Vec<_>>()
                .join("; ");
            if let Ok(v) = HeaderValue::from_str(&cookie_str) {
                headers.insert(COOKIE, v);
            }
        }

        // Custom headers
        for (k, v) in &self.custom_headers {
            if let (Ok(name), Ok(val)) = (HeaderName::from_bytes(k.as_bytes()), HeaderValue::from_str(v)) {
                headers.insert(name, val);
            }
        }

        headers
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_inferred_headers() {
        let session = SessionConfig::new().with_inferred_headers("https://example.com/video/stream.m3u8");
        assert_eq!(session.origin.as_deref(), Some("https://example.com"));
        assert_eq!(session.referer.as_deref(), Some("https://example.com/video/stream.m3u8"));

        let headers = session.build_headers();
        assert!(headers.contains_key(USER_AGENT));
        assert!(headers.contains_key(REFERER));
        assert!(headers.contains_key(ORIGIN));
    }

    #[test]
    fn test_cookie_parsing() {
        let mut session = SessionConfig::new();
        session.parse_cookie_header("session_token=abc123xyz; user_id=456; theme=dark");
        assert_eq!(session.cookies.get("session_token").map(String::as_str), Some("abc123xyz"));
        assert_eq!(session.cookies.get("user_id").map(String::as_str), Some("456"));

        let headers = session.build_headers();
        assert!(headers.contains_key(COOKIE));
    }
}
