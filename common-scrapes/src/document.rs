use std::fmt::Display;

use serde_derive::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Copy, Serialize, Deserialize)]
pub enum FileType {
    OriginalPDF,
    DigitalPDF,
    HTML,
    Unknown,
}

impl Display for FileType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Document {
    pub title: String,
    pub link: String,
    pub ftype: FileType,
}

impl Document {
    pub fn save<C: AsRef<[u8]>>(
        &self,
        save_to: &str,
        contents: C,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let path = std::path::PathBuf::from(format!("{save_to}{}", self.link));
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(path, contents)?;
        Ok(())
    }
}
