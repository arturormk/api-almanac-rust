use crate::error::RunnerError;
use crate::response::HttpResponse;
use api_almanac_model::ResolvedRequest;
use std::collections::HashMap;
use std::time::Instant;

pub struct Runner {
    client: reqwest::Client,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .expect("failed to build reqwest client"),
        }
    }

    pub async fn run(&self, req: &ResolvedRequest) -> Result<HttpResponse, RunnerError> {
        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| RunnerError::InvalidMethod(req.method.clone()))?;

        let mut rb = self.client.request(method, &req.url);

        for (k, v) in &req.headers {
            rb = rb.header(k.as_str(), v.as_str());
        }

        for (k, v) in &req.query {
            rb = rb.query(&[(k.as_str(), v.as_str())]);
        }

        if let Some(body) = &req.body {
            rb = rb
                .header("Content-Type", body.content_type)
                .body(body.content.clone());
        }

        let start = Instant::now();
        let response = rb.send().await?;
        let duration_ms = start.elapsed().as_millis() as u64;

        let status = response.status().as_u16();
        let status_text = response
            .status()
            .canonical_reason()
            .unwrap_or("")
            .to_string();
        let url = response.url().to_string();

        // Join multi-value headers with ", "
        let mut headers: HashMap<String, Vec<String>> = HashMap::new();
        for (k, v) in response.headers() {
            headers
                .entry(k.to_string())
                .or_default()
                .push(v.to_str().unwrap_or("").to_string());
        }
        let headers: HashMap<String, String> = headers
            .into_iter()
            .map(|(k, vals)| (k, vals.join(", ")))
            .collect();

        let body_bytes = response.bytes().await?;
        let body = String::from_utf8_lossy(&body_bytes).into_owned();

        Ok(HttpResponse {
            status,
            status_text,
            headers,
            body,
            duration_ms,
            url,
        })
    }
}

impl Default for Runner {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use api_almanac_model::{BodyKind, ResolvedBody, ResolvedRequest};

    fn simple_get(url: &str) -> ResolvedRequest {
        ResolvedRequest {
            id: "test".into(),
            name: "Test".into(),
            method: "GET".into(),
            url: url.to_string(),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: None,
        }
    }

    #[tokio::test]
    async fn get_request_returns_200() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("GET", "/ping")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"ok":true}"#)
            .create_async()
            .await;

        let runner = Runner::new();
        let req = simple_get(&format!("{}/ping", server.url()));
        let resp = runner.run(&req).await.unwrap();

        assert_eq!(resp.status, 200);
        assert_eq!(resp.status_text, "OK");
        assert_eq!(resp.body, r#"{"ok":true}"#);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn post_json_body_is_sent() {
        let mut server = mockito::Server::new_async().await;
        let mock = server
            .mock("POST", "/users")
            .with_status(201)
            .match_header("content-type", "application/json")
            .match_body(mockito::Matcher::PartialJsonString(
                r#"{"name":"Ada"}"#.to_string(),
            ))
            .with_body(r#"{"id":"usr_1","name":"Ada"}"#)
            .create_async()
            .await;

        let runner = Runner::new();
        let req = ResolvedRequest {
            id: "test".into(),
            name: "Test".into(),
            method: "POST".into(),
            url: format!("{}/users", server.url()),
            headers: HashMap::new(),
            query: HashMap::new(),
            body: Some(ResolvedBody {
                kind: BodyKind::Json,
                content: r#"{"name":"Ada"}"#.to_string(),
                content_type: "application/json",
            }),
        };

        let resp = runner.run(&req).await.unwrap();
        assert_eq!(resp.status, 201);
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn query_params_are_appended() {
        let mut server = mockito::Server::new_async().await;
        // Match path + any query string — order varies because source is a HashMap
        let mock = server
            .mock("GET", mockito::Matcher::Regex(r"^/search\?".to_string()))
            .with_status(200)
            .with_body("[]")
            .create_async()
            .await;

        let runner = Runner::new();
        let req = ResolvedRequest {
            id: "test".into(),
            name: "Test".into(),
            method: "GET".into(),
            url: format!("{}/search", server.url()),
            headers: HashMap::new(),
            query: [
                ("q".to_string(), "rust".to_string()),
                ("limit".to_string(), "10".to_string()),
            ]
            .into(),
            body: None,
        };

        let resp = runner.run(&req).await.unwrap();
        assert_eq!(resp.status, 200);
        assert!(resp.url.contains("q=rust"), "final url should contain q=rust");
        assert!(resp.url.contains("limit=10"), "final url should contain limit=10");
        mock.assert_async().await;
    }

    #[tokio::test]
    async fn non_200_status_is_returned_not_errored() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/not-found")
            .with_status(404)
            .with_body(r#"{"error":"not found"}"#)
            .create_async()
            .await;

        let runner = Runner::new();
        let req = simple_get(&format!("{}/not-found", server.url()));
        let resp = runner.run(&req).await.unwrap();

        assert_eq!(resp.status, 404);
        assert!(resp.body.contains("not found"));
    }

    #[tokio::test]
    async fn duration_is_measured() {
        let mut server = mockito::Server::new_async().await;
        server
            .mock("GET", "/")
            .with_status(200)
            .with_body("")
            .create_async()
            .await;

        let runner = Runner::new();
        let req = simple_get(&server.url());
        let resp = runner.run(&req).await.unwrap();
        assert!(resp.duration_ms < 5000, "duration should be sane");
    }
}
