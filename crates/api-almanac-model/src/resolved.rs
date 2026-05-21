use crate::body::BodyKind;
use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct ResolvedRequest {
    pub id: String,
    pub name: String,
    pub method: String,
    pub url: String,
    pub headers: HashMap<String, String>,
    pub query: HashMap<String, String>,
    pub body: Option<ResolvedBody>,
}

#[derive(Debug, Clone)]
pub struct ResolvedBody {
    pub kind: BodyKind,
    /// Serialized content ready to be sent as an HTTP body.
    pub content: String,
    /// MIME type inferred from kind.
    pub content_type: &'static str,
}
