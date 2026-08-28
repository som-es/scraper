use std::time::{Duration, Instant};

use chromiumoxide::{
    browser::{Browser, BrowserConfig},
    Page,
};
use futures_util::StreamExt;
use tokio::time::sleep;

use crate::error::ScraperError;

pub const WAF_TIMEOUT: Duration = Duration::from_secs(3);

pub enum Content {
    Token(String),
    Text(String),
}

impl From<chromiumoxide::error::CdpError> for ScraperError {
    fn from(error: chromiumoxide::error::CdpError) -> Self {
        Self::Other(format!("Chromium CDP error: {error}"))
    }
}

pub async fn solve_aws_waf_challenge(
    url: &str,
    timeout: Duration,
) -> Result<Content, ScraperError> {
    let config = BrowserConfig::builder()
        .no_sandbox()
        .build()
        .map_err(|error| ScraperError::Other(format!("Could not configure Chromium: {error}")))?;

    let (mut browser, mut handler) = Browser::launch(config)
        .await
        .map_err(|error| ScraperError::Other(format!("Could not launch Chromium: {error}")))?;

    let handler_task = tokio::spawn(async move {
        while let Some(event) = handler.next().await {
            if let Err(error) = event {
                log::error!("Chromium handler error: {error:}");
                break;
            }
        }
    });

    let result = solve_waf_challenge(&browser, url, timeout).await;

    handler_task.abort();

    if let Err(error) = browser.close().await {
        log::warn!("Failed to close Chromium: {error:?}");
    }

    result
}

async fn solve_waf_challenge(
    browser: &Browser,
    url: &str,
    timeout: Duration,
) -> Result<Content, ScraperError> {
    let page = browser
        .new_page("about:blank")
        .await
        .map_err(|error| ScraperError::Other(format!("Could not create Chromium page: {error}")))?;

    page.goto(url)
        .await
        .map_err(|error| ScraperError::Other(format!("Could not navigate to {url}: {error}")))?;

    let deadline = Instant::now() + timeout;

    loop {
        if Instant::now() >= deadline {
            let text = page
                .content()
                .await
                .map_err(|e| ScraperError::Other(format!("Could not get page content: {e}")))?;
            return Ok(Content::Text(text));
        }

        if let Some(token) = get_waf_token(&page).await? {
            return Ok(Content::Token(token));
        }

        sleep(Duration::from_millis(100)).await;
    }
}

async fn get_waf_token(page: &Page) -> Result<Option<String>, ScraperError> {
    let cookies = page.get_cookies().await.map_err(|error| {
        ScraperError::Other(format!("Could not retrieve Chromium cookies: {error}"))
    })?;

    Ok(cookies
        .into_iter()
        .find(|cookie| cookie.name == "aws-waf-token")
        .map(|cookie| cookie.value))
}

#[tokio::test]
async fn test_solve_aws_waf() {
    let url = "https://www.europarl.europa.eu/erpl-public/hemicycle/str/en.json";

    let content = solve_aws_waf_challenge(url, WAF_TIMEOUT).await.unwrap();

    match content {
        Content::Token(t) => println!("Obtained WAF token: {}", t),
        Content::Text(t) => println!("Page content: {}", t),
    }
}
