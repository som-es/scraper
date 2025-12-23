use common_scrapes::{Document, FileType};
use serde_json::Value;

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

/*
#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize)]
pub enum FileType {
    OriginalPDF,
    DigitalPDF,
    HTML,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub link: String,
    pub ftype: FileType,
}

impl Document {
    pub fn fetch(&self, base_url: &str) -> reqwest::Result<String> {
        let url = format!("{}{}", base_url, self.link);
        let response = reqwest::blocking::get(&url)?;
        response.text()
    }

    pub fn save(&self, save_to: &str, base_url: &str) -> Result<(), Box<dyn std::error::Error>> {
        let path = std::path::PathBuf::from(format!("{save_to}{}", self.link));
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, self.fetch(base_url)?)?;
        Ok(())
    }
}
*/
pub fn documents(raw_documents: &[Value]) -> Option<Vec<Document>> {
    let mut documents = vec![];

    for document in raw_documents {
        let title = document["title"].as_str().unwrap_or_default().to_string();
        let raw_documents = document["documents"].as_array()?;

        for document in raw_documents {
            let link = document["link"].as_str()?.to_string();
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
            .flat_map(|nr_phase| Some(documents(nr_phase["documents"].as_array()?)))
            .flatten()
            .flatten()
            .collect(),
    )
}
