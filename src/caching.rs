use std::{path::PathBuf, time::SystemTime};

use serde_json::Value;

#[inline]
fn cache_dir() -> String {
    std::env::var("SCRAPER_CACHE_DIR").unwrap_or("./caching".into())
}

#[inline]
fn ignore_cache_xxvii() -> bool {
    std::env::var("SCRAPER_IGNORE_CACHE_XXVII").unwrap_or("true".into()) == "true"
}

pub async fn get_disk_cache(entry: &str, redo_secs: u64) -> Option<String> {
    if entry.contains("XXVIII/") && ignore_cache_xxvii() {
        //     // ignores cache
        return None;
    }
    let cache_path = format!("{}{entry}.json", cache_dir());
    if !tokio::fs::try_exists(&cache_path).await.ok()? {
        return None;
    }
    let last_modified = tokio::fs::metadata(&cache_path)
        .await
        .ok()?
        .modified()
        .ok()?;
    let last_modified_duration = SystemTime::now().duration_since(last_modified).ok()?;
    if last_modified_duration.as_secs() > redo_secs {
        return None;
    }

    tokio::fs::read_to_string(cache_path).await.ok()
}

pub async fn disk_caching(entry: &str, data: impl AsRef<[u8]>) -> std::io::Result<()> {
    let cache_path = format!("{}{entry}.json", cache_dir());
    let cache_path = PathBuf::from(cache_path);
    tokio::fs::create_dir_all(&cache_path.parent().unwrap()).await?;
    tokio::fs::write(cache_path, data).await.unwrap();
    Ok(())
}

pub async fn get_disk_cache_json(entry: &str) -> Option<Value> {
    get_disk_cache(entry, 60 * 60 * 24 * 190)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}

pub async fn get_disk_cache_json_with_secs(entry: &str, redo_secs: u64) -> Option<Value> {
    get_disk_cache(entry, redo_secs)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}
