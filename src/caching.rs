use std::{future::Future, path::PathBuf, time::SystemTime};

use rand::distr::{weighted::WeightedIndex, Distribution};
use serde_json::Value;

#[inline]
pub(crate) fn cache_dir() -> String {
    std::env::var("SCRAPER_CACHE_DIR").unwrap_or("./caching".into())
}

#[derive(Debug, Clone)]
pub struct CacheConfig {
    pub cache_dir: String,
    pub ignore_current_gp_cache: bool,
    pub redo_secs: u64,
}

impl CacheConfig {
    pub fn new(cache_dir: String, redo_secs: u64, ignore_current_gp_cache: bool) -> Self {
        CacheConfig {
            cache_dir,
            ignore_current_gp_cache,
            redo_secs,
        }
    }

    pub fn cache_dir(&self) -> &str {
        &self.cache_dir
    }
}

impl Default for CacheConfig {
    fn default() -> Self {
        let redo_secs = {
            let days = weighted_rand(12, 90, 1.7);
            60 * 60 * 24 * days
        };
        Self {
            cache_dir: cache_dir(),
            redo_secs,
            ignore_current_gp_cache: ignore_cache_xxvii(),
        }
    }
}

#[inline]
fn ignore_cache_xxvii() -> bool {
    std::env::var("SCRAPER_IGNORE_CACHE_XXVII").unwrap_or("true".into()) == "true"
}

pub async fn get_disk_cache_parsed<T, O: Future<Output = (Option<T>, Option<u64>)>>(
    entry: &str,
    cache_config: &CacheConfig,
    redo_secs_cb: impl Fn(Value) -> O,
) -> Option<T> {
    if entry.contains("XXVIII/") && cache_config.ignore_current_gp_cache {
        //     // ignores cache
        return None;
    }
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .map(|idx| &entry[idx + 2..]) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
    let cache_path = format!("{}{entry}.json", cache_config.cache_dir);
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
    let redo_secs = new_redo_secs.unwrap_or(cache_config.redo_secs);

    let last_modified_duration = SystemTime::now().duration_since(last_modified).ok()?;
    if last_modified_duration.as_secs() > redo_secs {
        return None;
    }

    data
}

pub async fn get_disk_cache(entry: &str, cache_config: &CacheConfig) -> Option<String> {
    if entry.contains("XXVIII/") && cache_config.ignore_current_gp_cache {
        return None;
    }
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .map(|idx| &entry[idx + 2..]) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
    let cache_path = format!("{}{entry}.json", cache_config.cache_dir());
    if !tokio::fs::try_exists(&cache_path).await.ok()? {
        return None;
    }
    let last_modified = tokio::fs::metadata(&cache_path)
        .await
        .ok()?
        .modified()
        .ok()?;
    let last_modified_duration = SystemTime::now().duration_since(last_modified).ok()?;
    if last_modified_duration.as_secs() > cache_config.redo_secs {
        return None;
    }

    tokio::fs::read_to_string(cache_path).await.ok()
}

pub async fn disk_caching(
    entry: &str,
    data: impl AsRef<[u8]>,
    cache_config: &CacheConfig,
) -> std::io::Result<()> {
    let entry = if entry.starts_with("http") {
        entry
            .find("://")
            .map(|idx| &entry[idx + 2..]) // +2, skip :/, keep starting with /
            .unwrap_or(entry)
    } else {
        entry
    };
    let cache_path = format!("{}{entry}.json", cache_config.cache_dir());
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

pub async fn get_disk_cache_json(entry: &str, cache_config: &CacheConfig) -> Option<Value> {
    get_disk_cache(entry, cache_config)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}

pub async fn get_disk_cache_json_with_secs(
    entry: &str,
    cache_config: &CacheConfig,
) -> Option<Value> {
    get_disk_cache(entry, cache_config)
        .await
        .map(|x| serde_json::from_str(&x).ok())?
}
