use chrono::NaiveTime;
use serde_derive::{Deserialize, Serialize};

use crate::Opinion;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlenarSpeech {
    pub ty: String,
    pub duration_in_seconds: Option<u32>,
    pub start: Option<NaiveTime>,
    pub speech: Speech,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct SpeechTextInfo {
    pub site_start: Option<u64>,
    pub site_end: Option<u64>,
    pub document_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
pub struct Speech {
    pub delegate_id: u64,
    pub delegate_display_name: String,
    pub opinion: Opinion,
    pub spoken_text_html_urls: Vec<SpeechTextInfo>,
}
