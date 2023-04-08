use std::fmt::Display;

pub struct Url<'a> {
    pub base_url: &'a str,
}

impl<'a> Url<'a> {
    pub fn new(url: &str) -> Url {
        let base_url = url.trim_end_matches('/').trim_start_matches('/');

        Url { base_url }
    }

    #[inline]
    pub fn remove_first_slash(&self) -> &str {
        self.base_url.trim_start_matches('/')
    }
}

impl<'a> Display for Url<'a> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.base_url)
    }
}
