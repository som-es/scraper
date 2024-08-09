use std::sync::Arc;

use crate::error::ScraperError;
use reqwest::{Client, Method, StatusCode};
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
        .header("user-agent", "somes/2023.1.0")
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
        match res.status() {
            StatusCode::UNAUTHORIZED => Err(ScraperError::Unauthorized),
            StatusCode::FORBIDDEN => Err(ScraperError::Forbidden),
            StatusCode::NOT_FOUND => Err(ScraperError::NotFound),
            StatusCode::INTERNAL_SERVER_ERROR => Err(ScraperError::InternalServerError),
            StatusCode::UNPROCESSABLE_ENTITY => Err(ScraperError::Unauthorized),
            s => Err(ScraperError::StatusError(s)),
        }
    }
}
