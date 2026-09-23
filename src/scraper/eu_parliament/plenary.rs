use std::collections::HashMap;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use common_scrapes::{Document, FileType, Opinion, PlenarSpeech, SpeechTextInfo};
use regex::Regex;
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

use crate::{
    caching::CacheConfig,
    error::ScraperError,
    eu_parliament::votes::reference_from_report,
    language::{Language, Text},
    plenar::{Absence, Absences, Debate, DebateInfo, Plenar, PlenarInfo},
    text_utils::remove_paran_contents,
};

use super::{
    id_to_u64, last_path_segment, proposals::extract_documents_from_doc, EuParliamentScraper,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlenarySitting {
    pub id: String,
    pub date: NaiveDate,
    pub term: Option<String>,
    pub title: Option<String>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub locality: Option<String>,
    pub number_of_attendees: Option<u32>,
    pub attendee_ids: Vec<u64>,
    pub excused_ids: Vec<u64>,
    pub debate_item_ids: Vec<String>,
    pub vote_item_ids: Vec<String>,
    pub agenda_doc_ids: Vec<String>,
    pub attendance_doc_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speech {
    pub id: String,
    pub sitting_date: NaiveDate,
    pub is_part_of: Option<String>,
    pub title: Option<String>,
    pub speaker_id: Option<u64>,
    pub speaker_group: Option<String>,
    pub role: Option<String>,
    pub start: Option<DateTime<FixedOffset>>,
    pub end: Option<DateTime<FixedOffset>>,
    pub verbatim_doc_id: Option<String>,
    pub original_language: Option<String>,
    pub agenda_item_id: Option<String>,
    pub numbering: u64,
    pub speech_texts: Vec<Text>,
    pub ty: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlenaryAgendaItem {
    pub id: String,
    pub title: Option<String>,
    pub activity_type: Option<String>,
    pub based_on_references: Vec<String>,
    pub speech_ids: Vec<String>,
    pub vote_ids: Vec<String>,
}

impl EuParliamentScraper {
    pub async fn get_plenary_sitting(
        &self,
        sitting_id: &str,
    ) -> Result<PlenarySitting, ScraperError> {
        let raw = self
            .get_one_cached(&format!("meetings/{sitting_id}"))
            .await?;
        parse_plenary_sitting(&raw).ok_or(ScraperError::ParseError)
    }

    pub async fn get_plenary_sitting_by_date(
        &self,
        date: NaiveDate,
    ) -> Result<PlenarySitting, ScraperError> {
        self.get_plenary_sitting(&Self::sitting_id(date)).await
    }

    pub async fn list_plenary_sitting_ids(&self, year: u32) -> Result<Vec<String>, ScraperError> {
        let rows = self
            .get_all_pages("meetings", &format!("year={year}"))
            .await?;
        let mut ids: Vec<String> = rows
            .into_iter()
            .flat_map(|row| {
                row["activity_id"]
                    .as_str()
                    .or_else(|| row["id"].as_str().map(last_path_segment))
                    .map(ToString::to_string)
            })
            .filter(|id| id.starts_with("MTG-PL-"))
            .collect();
        ids.sort();
        ids.dedup();
        Ok(ids)
    }

    pub async fn get_sitting_speeches(&self, date: NaiveDate) -> Result<Vec<Speech>, ScraperError> {
        let rows = self
            .get_all_pages_cached(
                "speeches",
                &format!("sitting-date={date}&include-output=xml_fragment"),
            )
            .await?;
        Ok(rows.iter().flat_map(parse_speech).collect())
    }

    pub async fn get_speech(&self, speech_id: &str) -> Result<Speech, ScraperError> {
        let raw = self
            .get_one_cached(&format!("speeches/{speech_id}"))
            .await?;
        parse_speech(&raw).ok_or(ScraperError::ParseError)
    }

    pub async fn get_agenda_item(&self, item_id: &str) -> Result<PlenaryAgendaItem, ScraperError> {
        let raw = self.get_one_cached(&format!("events/{item_id}")).await?;
        parse_agenda_item(&raw).ok_or(ScraperError::ParseError)
    }

    async fn plenary_debates(&self, speeches: &[Speech]) -> Vec<Debate> {
        let mut debates = Vec::new();
        for (item_id, item_speeches) in speeches_by_agenda_item(speeches) {
            let (about, references) = match self.get_agenda_item(&item_id).await {
                Ok(item) => (
                    item.title.unwrap_or_default(),
                    item.based_on_references
                        .iter()
                        .flat_map(|reference| reference_from_report(reference))
                        .collect(),
                ),
                Err(_) => (
                    item_speeches
                        .first()
                        .and_then(|speech| speech.title.clone())
                        .unwrap_or_default(),
                    vec![],
                ),
            };
            debates.push(Debate {
                info: DebateInfo { about, references },
                speeches: item_speeches
                    .into_iter()
                    .map(speech_to_plenar_speech)
                    .collect(),
            });
        }
        debates
    }

    async fn sitting_absences(&self, sitting: &PlenarySitting) -> Option<Absences> {
        if sitting.excused_ids.is_empty() {
            return None;
        }

        let attendance_doc_id = sitting.attendance_doc_id.clone().or_else(|| {
            sitting
                .term
                .as_ref()
                .map(|term| format!("PV-{term}-{}-ATT", sitting.date.format("%Y-%m-%d")))
        });

        let source_document = match &attendance_doc_id {
            Some(doc_id) => self
                .get_one_cached(&format!("documents/{doc_id}"))
                .await
                .ok()
                .as_ref()
                .and_then(attendance_document_from_raw),
            None => None,
        }
        .unwrap_or_else(|| Document {
            title: "attendance".to_string(),
            link: match &attendance_doc_id {
                Some(doc_id) => {
                    format!("{}/documents/{doc_id}", EuParliamentScraper::BASE_URL)
                }
                None => format!("{}/meetings/{}", EuParliamentScraper::BASE_URL, sitting.id),
            },
            ftype: FileType::Unknown,
        });

        Some(Absences {
            absences: sitting
                .excused_ids
                .iter()
                .map(|id| Absence {
                    name: "".into(),
                    id: Some(*id as u32),
                })
                .collect(),
            source_document,
            date: sitting.date,
        })
    }
}

#[async_trait]
impl Plenar for EuParliamentScraper {
    const FILTER_PATH: &'static str = "";

    fn plenar_payload(&self) -> Self::Payload {
        Value::Null
    }
    async fn post_plenar_payload(&self) -> Result<Value, ScraperError> {
        let year = &self.scrape_info.gp[0];
        let rows = self
            .get_all_pages("meetings", &format!("year={year}"))
            .await?;
        Ok(Value::Array(rows))
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<Vec<String>, ScraperError> {
        let rows = plenar_infos["data"]
            .as_array()
            .or_else(|| plenar_infos.as_array())
            .ok_or(ScraperError::PlenarGegenstandPathsParseError)?;
        Ok(rows
            .iter()
            .flat_map(|row| {
                row["activity_id"]
                    .as_str()
                    .or_else(|| row["id"].as_str().map(last_path_segment))
                    .map(ToString::to_string)
            })
            .filter(|id| id.starts_with("MTG-PL-"))
            .collect())
    }

    async fn raw_plenar_into_plenar_info(
        raw_plenar: Value,
        _cache_config: &CacheConfig,
    ) -> Option<PlenarInfo> {
        let sitting = parse_plenary_sitting(&raw_plenar)?;
        Some(sitting_to_plenar_info(&sitting))
    }

    async fn get_plenar(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let sitting_id = last_path_segment(plenar_path);
        let raw = self
            .get_one_cached(&format!("meetings/{sitting_id}"))
            .await?;
        let sitting =
            parse_plenary_sitting(&raw).ok_or(ScraperError::PlenarGegenstandParseError)?;

        let mut info = sitting_to_plenar_info(&sitting);
        let speeches = self
            .get_sitting_speeches(sitting.date)
            .await
            .unwrap_or_default();
        info.debates = self.plenary_debates(&speeches).await;
        info.absences = self
            .sitting_absences(&sitting)
            .await
            .map(|absences| vec![absences])
            .unwrap_or_default();
        Ok(info)
    }
}

fn sitting_to_plenar_info(sitting: &PlenarySitting) -> PlenarInfo {
    let created_at = sitting
        .start
        .map(|start| start.with_timezone(&Utc))
        .unwrap_or_else(|| sitting.date.and_hms_opt(0, 0, 0).unwrap().and_utc());
    let updated_at = sitting
        .end
        .map(|end| end.with_timezone(&Utc))
        .filter(|end| *end != created_at);

    PlenarInfo {
        title: sitting.title.clone().unwrap_or_default(),
        description: sitting.title.clone().unwrap_or_default(),
        ityp: "PLENARY_SITTING".to_string(),
        council: "ep".to_string(),
        inr: sitting
            .date
            .format("%Y%m%d")
            .to_string()
            .parse::<u64>()
            .unwrap_or(0),
        debates: vec![],
        created_at,
        updated_at,
        call_to_orders: vec![],
        gp: sitting.term.clone().unwrap_or_default(),
        absences: vec![],
        plenary_dates: vec![sitting.date],
        agenda_dates: vec![sitting.date],
        document_link_speeches: vec![],
    }
}

fn speech_to_plenar_speech(speech: Speech) -> PlenarSpeech {
    let duration_in_seconds = match (speech.start, speech.end) {
        (Some(start), Some(end)) => u32::try_from((end - start).num_seconds()).ok(),
        _ => None,
    };

    const DOC_URL: &str = "https://www.europarl.europa.eu/doceo/document/";

    let spoken_text_html_urls = speech
        .is_part_of
        .map(|is_part_of| SpeechTextInfo {
            document_url: format!("{DOC_URL}{is_part_of}_EN.html"), // TODO: mind language
            site_start: None,
            site_end: None,
        })
        .into_iter()
        .collect::<Vec<_>>();

    PlenarSpeech {
        duration_in_seconds,
        start: speech.start.map(|start| start.time()),
        speech: common_scrapes::Speech {
            delegate_id: speech.speaker_id.unwrap_or_default(),
            delegate_display_name: String::new(),
            opinion: Opinion::None,
            spoken_text_html_urls,
            speech_texts: Some(speech.speech_texts),
        },
        ty: speech.ty.unwrap_or_else(|| "none".into()),
    }
}

fn attendance_document_from_raw(raw: &Value) -> Option<Document> {
    let documents = extract_documents_from_doc(raw);
    let mut document = documents
        .iter()
        .find(|document| document.ftype == FileType::DigitalPDF)
        .or_else(|| {
            documents
                .iter()
                .find(|document| document.ftype == FileType::HTML)
        })
        .or_else(|| documents.first())
        .cloned()?;
    document.title = "attendance".to_string();
    Some(document)
}
pub fn speeches_by_agenda_item(speeches: &[Speech]) -> Vec<(String, Vec<Speech>)> {
    let mut grouped: HashMap<&str, Vec<Speech>> = HashMap::new();
    for speech in speeches {
        let Some(key) = speech.agenda_item_id.as_deref() else {
            continue;
        };
        grouped
            .entry(key)
            .or_insert_with(Vec::new)
            .push(speech.clone());
    }

    grouped
        .into_iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect()
}

fn parse_dt(value: &Value) -> Option<DateTime<FixedOffset>> {
    DateTime::parse_from_rfc3339(value.as_str()?).ok()
}

fn english_label(raw: &Value) -> Option<String> {
    raw["activity_label"]["en"]
        .as_str()
        .map(ToString::to_string)
}

fn person_ids(value: &Value) -> Vec<u64> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry.as_str())
        .flat_map(id_to_u64)
        .collect()
}

fn sub_activity_ids(raw: &Value, needle: &str) -> Vec<String> {
    raw["consists_of"]
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry.as_str().or_else(|| entry["id"].as_str()))
        .map(last_path_segment)
        .filter(|id| id.contains(needle))
        .map(ToString::to_string)
        .collect()
}

fn string_array(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .flat_map(|entry| entry.as_str().or_else(|| entry["id"].as_str()))
        .map(ToString::to_string)
        .collect()
}

pub fn parse_plenary_sitting(raw: &Value) -> Option<PlenarySitting> {
    let id = raw["activity_id"]
        .as_str()
        .or_else(|| raw["id"].as_str().map(last_path_segment))?
        .to_string();
    let date = NaiveDate::parse_from_str(
        raw["activity_date"].as_str().or_else(|| {
            raw["activity_start_date"]
                .as_str()
                .map(|raw| raw.split('T').next())
                .flatten()
        })?,
        "%Y-%m-%d",
    )
    .ok()?;

    Some(PlenarySitting {
        id,
        date,
        term: raw["parliamentary_term"].as_str().map(|term| {
            last_path_segment(term)
                .trim_start_matches("ep-")
                .to_string()
        }),
        title: english_label(raw),
        start: parse_dt(&raw["activity_start_date"]),
        end: parse_dt(&raw["activity_end_date"]),
        locality: raw["hasLocality"]
            .as_str()
            .map(|loc| last_path_segment(loc).to_string()),
        number_of_attendees: raw["number_of_attendees"].as_u64().map(|n| n as u32),
        attendee_ids: person_ids(&raw["had_participant_person"]),
        excused_ids: person_ids(&raw["had_excused_person"]),
        debate_item_ids: sub_activity_ids(raw, "-PVCRE-ITM-"),
        vote_item_ids: sub_activity_ids(raw, "-VOT-ITM-"),
        agenda_doc_ids: string_array(&raw["documented_by_a_realization_of"])
            .into_iter()
            .map(|id| last_path_segment(&id).to_string())
            .collect(),
        attendance_doc_id: string_array(&raw["recorded_in_a_realization_of"])
            .into_iter()
            .map(|id| last_path_segment(&id).to_string())
            .find(|id| id.contains("-ATT")),
    })
}

pub fn parse_speech(raw: &Value) -> Option<Speech> {
    let id = raw["activity_id"]
        .as_str()
        .or_else(|| raw["id"].as_str().map(last_path_segment))?
        .to_string();
    let sitting_date =
        NaiveDate::parse_from_str(raw["activity_date"].as_str()?, "%Y-%m-%d").ok()?;

    let participation = &raw["had_participation"];
    let speaker_id = participation["had_participant_person"]
        .as_array()
        .and_then(|people| people.first())
        .and_then(|person| person.as_str())
        .and_then(id_to_u64);
    let speaker_group = participation["participation_in_name_of"]
        .as_str()
        .map(|group| last_path_segment(group).to_string());
    let role = participation["participation_role"]
        .as_str()
        .map(|role| last_path_segment(role).to_string());

    let recorded = &raw["recorded_in_a_realization_of"];
    let record = recorded
        .as_array()
        .and_then(|records| records.first())
        .unwrap_or(recorded);

    let numbering = record.get("numbering")?.as_u64()?;
    let verbatim_doc_id = record["id"]
        .as_str()
        .or_else(|| record.as_str())
        .map(|id| last_path_segment(id).to_string());
    let original_language = record["originalLanguage"]
        .as_array()
        .and_then(|langs| langs.first())
        .and_then(|lang| lang.as_str())
        .map(|lang| last_path_segment(lang).to_string());

    let mut ty = None;
    let xml_fragments = record.get("api:xmlFragment");
    let speech_texts = xml_fragments.map(|xml_fragment| {
        Language::ALL
            .into_iter()
            .flat_map(|language| {
                let xml_text = xml_fragment.get(language.as_str())?.as_str()?;
                let (removed_content, text) = remove_paran_contents(&xml_text, '<', '>');
                let is_oral_statment = removed_content
                    .iter()
                    .any(|content| content.content.contains("oralStatements"));

                let is_written_statement = removed_content
                    .iter()
                    .any(|content| content.content.contains("writtenStatements"));

                if is_oral_statment {
                    ty = Some("wm")
                }
                if is_written_statement {
                    ty = Some("ws")
                }

                Some(Text { language, text })
            })
            .collect::<Vec<_>>()
    });
    let is_part_of = record["is_part_of"]
        .as_str()
        .or_else(|| record.as_str())
        .map(|id| last_path_segment(id).to_string());

    let agenda_item_id = raw["inverse_consists_of"]
        .as_array()
        .and_then(|parents| parents.first())
        .and_then(|parent| parent.as_str().or_else(|| parent["id"].as_str()))
        .map(|id| last_path_segment(id).to_string());

    Some(Speech {
        id,
        is_part_of,
        sitting_date,
        title: english_label(raw),
        speaker_id,
        speaker_group,
        role,
        start: parse_dt(&raw["activity_start_date"]),
        end: parse_dt(&raw["activity_end_date"]),
        verbatim_doc_id,
        original_language,
        agenda_item_id,
        speech_texts: speech_texts.unwrap_or_default(),
        numbering,
        ty: ty.map(|ty| ty.to_string()),
    })
}

pub fn parse_agenda_item(raw: &Value) -> Option<PlenaryAgendaItem> {
    let id = raw["activity_id"]
        .as_str()
        .or_else(|| raw["id"].as_str().map(last_path_segment))?
        .to_string();

    Some(PlenaryAgendaItem {
        id,
        title: english_label(raw),
        activity_type: raw["had_activity_type"]
            .as_str()
            .map(|activity| last_path_segment(activity).to_string()),
        based_on_references: based_on_references(raw),
        speech_ids: sub_activity_ids(raw, "-OTH-"),
        vote_ids: sub_activity_ids(raw, "-DEC-"),
    })
}

fn based_on_references(raw: &Value) -> Vec<String> {
    let doc_id_regex = Regex::new(r"^eli/dl/doc/([A-Z]+)-(\d+)-(\d{4})-(\d+)$").unwrap();
    string_array(&raw["based_on_a_realization_of"])
        .iter()
        .flat_map(|id| {
            let captures = doc_id_regex.captures(id)?;
            let prefix = captures.get(1)?.as_str();
            let term = captures.get(2)?.as_str();
            let year = captures.get(3)?.as_str();
            let number = captures.get(4)?.as_str();
            Some(format!("{prefix}{term}-{number}/{year}"))
        })
        .collect()
}

#[cfg(test)]
mod tests;
