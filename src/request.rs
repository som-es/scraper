use std::{sync::Arc, time::Duration};

use crate::error::ScraperError;
use reqwest::{
    header::{HeaderMap, RETRY_AFTER},
    Client, Method, StatusCode,
};
use serde::Serialize;
use serde_json::Value;

pub async fn request<B, S: AsRef<str>>(
    method: Method,
    client: &Client,
    body: B,
    url: S,
) -> Result<Value, ScraperError>
where
    B: Serialize,
{
    let allow_body = method == reqwest::Method::POST || method == reqwest::Method::PUT;

    let mut req = client
        .request(method, url.as_ref())
        .header("user-agent", "somes/2026.1.0")
        .header("content-type", "application/json");

    if allow_body {
        req = req.json(&body);
    }

    let res = req.send().await;

    let res = match res {
        Ok(res) => res,
        Err(e) => return Err(ScraperError::RequestError(Arc::new(e))),
    };

    if res.status().is_success() {
        res.json::<Value>()
            .await
            .map_err(|e| ScraperError::RequestError(Arc::new(e)))
    } else {
        Err(status_error(res.status(), res.headers()))
    }
}

/// Plain `GET` returning the raw response body as text, for non-JSON resources (e.g.
/// the EU Parliament's DOCEO XML documents).
pub async fn request_text<S: AsRef<str>>(client: &Client, url: S) -> Result<String, ScraperError> {
    let res = client
        .get(url.as_ref())
        .header("user-agent", "somes/2026.1.0")
        .send()
        .await;

    let res = match res {
        Ok(res) => res,
        Err(e) => return Err(ScraperError::RequestError(Arc::new(e))),
    };

    if res.status().is_success() {
        res.text()
            .await
            .map_err(|e| ScraperError::RequestError(Arc::new(e)))
    } else {
        Err(status_error(res.status(), res.headers()))
    }
}

const MAX_BACKOFF_SECS: u64 = 320;

fn status_error(status: StatusCode, headers: &HeaderMap) -> ScraperError {
    match status {
        StatusCode::UNAUTHORIZED => ScraperError::Unauthorized,
        StatusCode::FORBIDDEN => ScraperError::Forbidden,
        StatusCode::NOT_FOUND => ScraperError::NotFound,
        StatusCode::INTERNAL_SERVER_ERROR => ScraperError::InternalServerError,
        StatusCode::UNPROCESSABLE_ENTITY => ScraperError::Unauthorized,
        StatusCode::TOO_MANY_REQUESTS => ScraperError::TooManyRequests(
            headers
                .get(RETRY_AFTER)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.trim().to_owned().parse::<u64>().ok())
                .flatten()
                .min(Some(MAX_BACKOFF_SECS))
                .map(|retry_after| Duration::from_secs(retry_after)),
        ),
        s => ScraperError::StatusError(s),
    }
}
