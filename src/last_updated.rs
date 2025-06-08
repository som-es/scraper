pub trait LastUpdated {
    fn last_updated(&self) -> chrono::NaiveDateTime;
}
