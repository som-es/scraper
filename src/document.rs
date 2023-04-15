use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy)]
pub enum FileType {
    OriginalPDF,
    DigitalPDF,
    HTML,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Document {
    pub title: String,
    pub link: String,
    pub ftype: FileType,
}

pub fn documents(raw_documents: &[Value]) -> Option<Vec<Document>> {
    let mut documents = vec![];

    for document in raw_documents {
        let title = document["title"].as_str()?.to_string();
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

            let title = title.clone();
            documents.push(Document { title, link, ftype })
        }
    }

    Some(documents)
}
