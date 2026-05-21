use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HttpResponse {
    /// HTTP status code, e.g. 200, 404.
    pub status: u16,
    /// Reason phrase, e.g. "OK", "Not Found".
    pub status_text: String,
    /// Response headers (multi-value headers joined with ", ").
    pub headers: HashMap<String, String>,
    /// Response body decoded as UTF-8. Non-UTF-8 bytes are replaced.
    pub body: String,
    /// Total round-trip duration in milliseconds.
    pub duration_ms: u64,
    /// Final URL after any redirects.
    pub url: String,
}
