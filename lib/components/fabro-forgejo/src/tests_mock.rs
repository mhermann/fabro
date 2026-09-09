//! Crate-internal test double: a scripted [`HttpClient`] shared by the
//! `lib.rs` and `tests` modules.

use crate::{HttpClient, HttpMethod, HttpResponse};

pub(crate) struct MockRoute {
    method:           HttpMethod,
    path:             String,
    status:           u16,
    response_body:    String,
    assert_header:    Option<(String, String)>,
    assert_body_json: Option<serde_json::Value>,
}

pub(crate) struct MockHttpClient {
    routes: Vec<MockRoute>,
}

impl MockHttpClient {
    pub(crate) fn new() -> Self {
        Self { routes: vec![] }
    }

    pub(crate) fn on(mut self, method: HttpMethod, path: &str, status: u16, body: &str) -> Self {
        self.routes.push(MockRoute {
            method,
            path: path.to_string(),
            status,
            response_body: body.to_string(),
            assert_header: None,
            assert_body_json: None,
        });
        self
    }

    pub(crate) fn with_req_header(mut self, name: &str, value: &str) -> Self {
        self.routes.last_mut().unwrap().assert_header = Some((name.to_string(), value.to_string()));
        self
    }

    pub(crate) fn with_req_body(mut self, json_str: &str) -> Self {
        self.routes.last_mut().unwrap().assert_body_json =
            Some(serde_json::from_str(json_str).unwrap());
        self
    }
}

impl HttpClient for MockHttpClient {
    async fn request(
        &self,
        method: HttpMethod,
        url: &str,
        headers: &[(&str, &str)],
        body: Option<&serde_json::Value>,
    ) -> anyhow::Result<HttpResponse> {
        // Match on the path without the query string so routes registered by
        // path also match URLs built with query pairs.
        let path = url.split('?').next().unwrap_or(url);
        for route in &self.routes {
            if method == route.method && path.ends_with(&route.path) {
                if let Some((name, expected)) = &route.assert_header {
                    let (_, v) = headers
                        .iter()
                        .find(|(k, _)| *k == name.as_str())
                        .unwrap_or_else(|| {
                            panic!("Expected header '{name}' not found in request to {url}")
                        });
                    assert_eq!(*v, expected.as_str(), "Header '{name}' mismatch for {url}");
                }
                if let Some(expected_body) = &route.assert_body_json {
                    let actual = body.expect("Expected request body");
                    assert_eq!(actual, expected_body, "Request body mismatch for {url}");
                }
                return Ok(HttpResponse::new(route.status, route.response_body.clone()));
            }
        }
        panic!(
            "No mock route for {:?} {url}\nRegistered routes: {:?}",
            method,
            self.routes
                .iter()
                .map(|r| format!("{:?} {}", r.method, r.path))
                .collect::<Vec<_>>()
        );
    }
}
