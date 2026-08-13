use async_trait::async_trait;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use common_scrapes::{Document, PlenarSpeech, Reference};
use serde_json::{json, Value};

mod absences;
mod call_to_orders;
mod speeches;
#[cfg(test)]
mod tests;

use crate::{
    caching::{disk_caching, get_disk_cache_json, CacheConfig},
    call_to_order::CallToOrderInfo,
    convert_vienna_datetime_to_utc, documents,
    error::ScraperError,
    plenar::{
        absences::extract_absences,
        call_to_orders::extract_call_to_order_info_from_stages,
        speeches::{combine_speech_info, extract_speeches},
    },
    speeches::extract_speeches_from_stages,
    AustriaScraper, GegenstandPaths, LastUpdated, Scraper,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Debate {
    pub info: DebateInfo,
    pub speeches: Vec<PlenarSpeech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct LegisUniqueInfo {
    pub legis_gp: String,
    pub legis_ityp: String,
    pub legis_inr: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct DebateInfo {
    pub about: String,
    pub references: Vec<Reference>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlenarInfo {
    pub title: String,
    pub description: String,
    pub ityp: String,
    pub council: String,
    pub inr: u64,
    pub debates: Vec<Debate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub call_to_orders: Vec<CallToOrderInfo>,
    pub gp: String,
    pub absences: Vec<Absences>,
    pub plenary_dates: Vec<NaiveDate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Absence {
    pub name: String,
    pub id: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct InnerAbsences {
    pub absences: Vec<Vec<Absence>>,
    pub source_document: Document,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Absences {
    pub absences: Vec<Absence>,
    pub source_document: Document,
    pub date: NaiveDate,
}

impl LastUpdated for PlenarInfo {
    fn last_updated(&self) -> DateTime<Utc> {
        self.updated_at.unwrap_or(chrono::Local::now().to_utc())
    }
}

#[async_trait]
pub trait Plenar: Scraper {
    const FILTER_PATH: &'static str;
    fn plenar_payload(&self) -> Self::Payload;

    async fn post_plenar_payload(&self) -> Result<Value, ScraperError> {
        self.post_payload_filter(self.plenar_payload(), Self::FILTER_PATH)
            .await
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn raw_plenar_into_plenar_info(
        raw_plenar: Value,
        cache_config: &CacheConfig,
    ) -> Option<PlenarInfo>;

    async fn get_plenar(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let raw = match get_disk_cache_json(plenar_path, self.cache_config()).await {
            Some(raw) => raw,
            None => {
                let raw_plenar = self.get_gegenstand_info(plenar_path).await?;
                if let Err(e) =
                    disk_caching(plenar_path, raw_plenar.to_string(), self.cache_config()).await
                {
                    log::warn!("could cache raw {plenar_path}: {e:?}")
                }
                raw_plenar
            }
        };

        Self::raw_plenar_into_plenar_info(raw, self.cache_config())
            .await
            .ok_or(ScraperError::PlenarGegenstandParseError)
    }
}

#[async_trait]
impl Plenar for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_007&listeId=undefined&showAll=true";

    fn plenar_payload(&self) -> Self::Payload {
        json!({"MODUS": ["PLENAR"], "GP": self.scrape_info.gp, "NRBRBV": self.scrape_info.council_type})
    }

    fn plenar_paths(&self, plenar_infos: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(plenar_infos, 8)
            .ok_or(ScraperError::ProposalParseError)
    }

    async fn raw_plenar_into_plenar_info(
        raw_plenar: Value,
        cache_config: &CacheConfig,
    ) -> Option<PlenarInfo> {
        let description = raw_plenar["meta"]["description"]
            .as_str()
            .unwrap()
            .to_string();

        // TODO: there is even more content! look in insomnia
        let plenar_content_0 = &raw_plenar["content"].as_array().unwrap()[0];

        let ityp = plenar_content_0["ityp"]
            .as_str()
            .unwrap_or("NRSITZ")
            .to_string();
        let inr = plenar_content_0["inr"].as_u64()?;

        let gp_code = plenar_content_0["gp_code"].as_str()?.to_string();

        let title = plenar_content_0["title"]
            .as_str()
            .unwrap_or(&description)
            .to_string();

        let created_at = NaiveDateTime::parse_from_str(
            plenar_content_0["einlangen"].as_str().unwrap(),
            "%Y-%m-%dT%H:%M:%S",
        )
        .ok()
        .map(convert_vienna_datetime_to_utc)??;

        let updated_at = plenar_content_0["update"].as_str();
        let updated_at = updated_at.and_then(|updated_at| {
            convert_vienna_datetime_to_utc(
                NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap(),
            )
        });

        let mut document_link_speeches = vec![];
        let mut call_to_orders = vec![];
        if let Some(stages) = plenar_content_0.get("stages") {
            document_link_speeches = extract_speeches_from_stages(stages).unwrap_or_default();
            call_to_orders = extract_call_to_order_info_from_stages(&ityp, inr, stages);
        }

        let past_debates = if let Some(content_1) = raw_plenar["content"].as_array()?.get(1) {
            content_1["past_debates"]
                .as_array()
                .map(|debates| debates.as_slice())
                .unwrap_or_default()
        } else {
            &[]
        };

        let raw_documents = plenar_content_0["documents"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let mut praesidium_documents = vec![];

        if let Some(content_2) = raw_plenar["content"].as_array()?.get(2) {
            if let Some(raw_praesidiums) = extract_praesidium_from_progress(&content_2["progress"])
            {
                for raw_praesidium in raw_praesidiums {
                    praesidium_documents.extend(
                        extract_praesidium_documents(raw_praesidium)
                            .unwrap_or_default()
                            .into_iter()
                            .map(|mut x| {
                                x.title = "stenographisches".to_string();
                                x
                            }),
                    )
                }
            }
        }
        let plenary_dates = extract_session_days(&raw_plenar);
        let documents = documents(&raw_documents, AustriaScraper::BASE_URL).unwrap_or_default();

        let mut absences_documents = praesidium_documents;
        let mut single_doc = false;
        if absences_documents.is_empty() {
            single_doc = true;
            absences_documents.extend(documents.iter().cloned())
        }

        let mut absences = extract_absences(&absences_documents, cache_config).await;

        let absences = if absences.is_empty() {
            if single_doc {
                let res = std::mem::take(absences.first_mut().unwrap());

                if res.absences.len() == plenary_dates.len() {
                    res.absences
                        .into_iter()
                        .zip(&plenary_dates)
                        .map(|(absences, date)| Absences {
                            absences,
                            source_document: res.source_document.clone(),
                            date: *date,
                        })
                        .collect()
                } else if !res.absences.is_empty() {
                    vec![Absences {
                        absences: res.absences[0].clone(),
                        source_document: res.source_document.clone(),
                        date: plenary_dates[0],
                    }]
                } else {
                    vec![]
                }
            } else {
                if absences.len() == plenary_dates.len() {
                    absences
                        .into_iter()
                        .zip(&plenary_dates)
                        .map(|(absence, date)| Absences {
                            absences: absence.absences[0].clone(),
                            source_document: absence.source_document,
                            date: *date,
                        })
                        .collect()
                } else if !absences.is_empty() {
                    let absence = std::mem::take(absences.first_mut().unwrap());
                    if absence.absences.is_empty() {
                        vec![]
                    } else {
                        vec![Absences {
                            absences: absence.absences[0].clone(),
                            source_document: absence.source_document.clone(),
                            date: plenary_dates[0],
                        }]
                    }
                } else {
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // path with no content at 1: /gegenstand/XXVII/NRSITZ/216

        let debates_with_speeches = extract_speeches(past_debates);
        let debates = combine_speech_info(debates_with_speeches, document_link_speeches);

        Some(PlenarInfo {
            title,
            description,
            ityp,
            inr,
            debates,
            call_to_orders,
            created_at,
            updated_at,
            council: if gp_code == "BR" {
                "BR".into()
            } else {
                "NR".into()
            },
            gp: gp_code,
            absences,
            plenary_dates,
        })
    }
}

fn extract_praesidium_from_progress(raw_progress: &Value) -> Option<Vec<&Value>> {
    Some(
        raw_progress
            .as_array()?
            .iter()
            .filter_map(|step| {
                if step["text"].as_str()?.to_lowercase().contains("präsidium") {
                    Some(step)
                } else {
                    None
                }
            })
            .collect(),
    )
}

fn extract_praesidium_documents(raw_praesidium: &Value) -> Option<Vec<Document>> {
    documents(
        raw_praesidium["protocol"]["data"]["links"].as_array()?,
        AustriaScraper::BASE_URL,
    )
}

fn extract_session_days(raw_plenar: &Value) -> Vec<NaiveDate> {
    raw_plenar["content"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("current_session"))
        .filter_map(|cs| cs.get("parts"))
        .filter_map(|parts| parts.as_array())
        .flatten()
        .filter_map(|part| part.get("date"))
        .filter_map(|date| date.as_str())
        .filter_map(|s| s.split('T').next())
        .filter_map(|s| NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .collect()
}
