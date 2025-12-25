use common_scrapes::{Document, FileType};
use serde_json::Value;

use crate::{AustriaScraper, Scraper};

pub trait Fetch {
    fn fetch(&self, base_url: &str) -> reqwest::Result<String>;
}

impl Fetch for Document {
    fn fetch(&self, base_url: &str) -> reqwest::Result<String> {
        let url = format!("{}{}", base_url, self.link);
        let response = reqwest::blocking::get(url)?;
        response.text()
    }
}

pub fn add_base_url_to_url(url: &str, base_url: &str) -> String {
    let mut url = url.to_string();
    if url.starts_with("http") {
        return url;
    }

    let base_url_has_slash = base_url.ends_with('/');
    let url_starts_with_slash = url.starts_with('/');

    match (base_url_has_slash, url_starts_with_slash) {
        (true, true) => {
            url = format!("{}{}", base_url, &url[1..]);
        }
        (false, false) => {
            url = format!("{}/{}", base_url, url);
        }
        _ => {
            url = format!("{}{}", base_url, url);
        }
    }

    url
}

pub fn documents(raw_documents: &[Value], base_url: &str) -> Option<Vec<Document>> {
    let mut documents = vec![];

    for document in raw_documents {
        let title = document["title"].as_str().unwrap_or_default().to_string();
        let raw_documents = document["documents"].as_array()?;

        for document in raw_documents {
            let link = add_base_url_to_url(document["link"].as_str()?, base_url);
            let ftype = document["type"].as_str()?;
            let ftype = if title.contains("Original") {
                FileType::OriginalPDF
            } else if ftype == "PDF" {
                FileType::DigitalPDF
            } else if ftype == "HTML" {
                FileType::HTML
            } else {
                FileType::Unknown
            };

            let title = title.clone().trim().to_string();
            documents.push(Document { title, link, ftype })
        }
    }

    Some(documents)
}

pub fn extract_plenar_documents(legis_content: &Value) -> Option<Vec<Document>> {
    Some(
        legis_content
            .get("phase")?
            .as_array()?
            .iter()
            .find(|phase| {
                phase["name"]
                    .as_str()
                    .unwrap()
                    .contains("Plenarberatungen NR")
            })
            .into_iter()
            .flat_map(|nr_phase| {
                Some(documents(
                    nr_phase["documents"].as_array()?,
                    AustriaScraper::BASE_URL,
                ))
            })
            .flatten()
            .flatten()
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_add_base_url_to_url() {
        let base_url = "https://example.com";
        let url1 = "/path/to/resource";
        let url2 = "path/to/resource";
        let url3 = "https://other.com/resource";
        assert_eq!(
            add_base_url_to_url(url1, base_url),
            "https://example.com/path/to/resource"
        );
        assert_eq!(
            add_base_url_to_url(url2, base_url),
            "https://example.com/path/to/resource"
        );
        assert_eq!(
            add_base_url_to_url(url3, base_url),
            "https://other.com/resource"
        );

        let base_url = "https://example.com/";
        assert_eq!(
            add_base_url_to_url(url1, base_url),
            "https://example.com/path/to/resource"
        );
        assert_eq!(
            add_base_url_to_url(url2, base_url),
            "https://example.com/path/to/resource"
        );
        assert_eq!(
            add_base_url_to_url(url3, base_url),
            "https://other.com/resource"
        );
    }
}
