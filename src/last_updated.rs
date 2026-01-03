pub trait LastUpdated {
    fn last_updated(&self) -> chrono::DateTime<chrono::Utc>;
}
