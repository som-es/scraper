use reqwest::{Client, Method, StatusCode};
use serde::Serialize;
use serde_json::Value;
use crate::error::ScraperError;

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

    let Ok(res) = res else {
        return Err(ScraperError::RequestError);
    };

    if res.status().is_success() {
        res.json::<Value>()
            .await
            .map_err(|_| ScraperError::RequestError)
    } else {
        match res.status() {
            StatusCode::UNAUTHORIZED => Err(ScraperError::Unauthorized),
            StatusCode::FORBIDDEN => Err(ScraperError::Forbidden),
            StatusCode::NOT_FOUND => Err(ScraperError::NotFound),
            StatusCode::INTERNAL_SERVER_ERROR => Err(ScraperError::InternalServerError),
            StatusCode::UNPROCESSABLE_ENTITY => Err(ScraperError::Unauthorized),
            _ => Err(ScraperError::RequestError),
        }
    }
}
