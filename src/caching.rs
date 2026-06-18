use std::{future::Future, path::PathBuf, time::SystemTime};

use rand::distr::{weighted::WeightedIndex, Distribution};
use serde_json::Value;

#[inline]
fn cache_dir() -> String {
    std::env::var("SCRAPER_CACHE_DIR").unwrap_or("./caching".into())
}

#[inline]
fn ignore_cache_xxvii() -> bool {
    std::env::var("SCRAPER_IGNORE_CACHE_XXVII").unwrap_or("true".into()) == "true"
}

pub async fn get_disk_cache_parsed<T, O: Future<Output = (Option<T>, Option<u64>)>>(
    entry: &str,
    redo_secs: u64,
    redo_secs_cb: impl Fn(Value) -> O,
) -> Option<T> {
    if entry.contains("XXVIII/") && ignore_cache_xxvii() {
        //     // ignores cache
        return None;
    }
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .and_then(|idx| Some(&entry[idx + 2..])) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
    let cache_path = format!("{}{entry}.json", cache_dir());
    if !tokio::fs::try_exists(&cache_path).await.ok()? {
        return None;
    }
    let last_modified = tokio::fs::metadata(&cache_path)
        .await
        .ok()?
        .modified()
        .ok()?;

    let cached = tokio::fs::read_to_string(cache_path).await.ok()?;
    let data = serde_json::from_str::<Value>(&cached).ok()?;
    let (data, new_redo_secs) = redo_secs_cb(data).await;
    let redo_secs = new_redo_secs.unwrap_or(redo_secs);

    let last_modified_duration = SystemTime::now().duration_since(last_modified).ok()?;
    if last_modified_duration.as_secs() > redo_secs {
        return None;
    }

    data
}

pub async fn get_disk_cache(entry: &str, redo_secs: u64) -> Option<String> {
    if entry.contains("XXVIII/") && ignore_cache_xxvii() {
        //     // ignores cache
        return None;
    }
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .and_then(|idx| Some(&entry[idx + 2..])) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
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
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .and_then(|idx| Some(&entry[idx + 2..])) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
    let cache_path = format!("{}{entry}.json", cache_dir());
    let cache_path = PathBuf::from(cache_path);
    tokio::fs::create_dir_all(&cache_path.parent().unwrap()).await?;
    tokio::fs::write(cache_path, data).await.unwrap();
    Ok(())
}

fn weighted_rand(min: u64, max: u64, pow: f64) -> u64 {
    let values: Vec<u64> = (min..=max).collect();

    let weights: Vec<f64> = values
        .iter()
        .map(|&x| (1.0 + 3.0 * (x - min) as f64 / (max - min) as f64).powf(pow))
        .collect();
    let dist = WeightedIndex::new(weights).unwrap();
    let mut rng = rand::rng();

    values[dist.sample(&mut rng)]
}

pub async fn get_disk_cache_json(entry: &str) -> Option<Value> {
    let days = weighted_rand(12, 90, 1.7);
    get_disk_cache(entry, 60 * 60 * 24 * days)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}

pub async fn get_disk_cache_json_with_secs(entry: &str, redo_secs: u64) -> Option<Value> {
    get_disk_cache(entry, redo_secs)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}
