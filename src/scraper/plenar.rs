use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use common_scrapes::{Document, FileType, Opinion, PlenarSpeech, Reference, Speech};
use regex::Regex;
use serde_json::{json, Value};

use crate::{
    caching::{disk_caching, get_disk_cache_json},
    call_to_order::{
        extract_call_to_order_id_from_path, extract_call_to_order_paths_from_stages,
        CallToOrderInfo,
    },
    documents,
    error::ScraperError,
    filter::filter_name_extraction,
    person_url_to_id,
    speeches::extract_speeches_from_stages,
    unique_titles, AustriaScraper, GegenstandPaths, LastUpdated, Scraper, TextExtractor,
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
    pub inr: u64,
    pub debates: Vec<Debate>,
    pub date: NaiveDateTime,
    pub updated_at: Option<NaiveDateTime>,
    pub call_to_orders: Vec<CallToOrderInfo>,
    pub gp: String,
    pub absences: Option<Vec<String>>,
}

impl LastUpdated for PlenarInfo {
    fn last_updated(&self) -> chrono::NaiveDateTime {
        self.updated_at
            .unwrap_or(chrono::Local::now().naive_local())
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

    async fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo>;

    async fn get_plenar(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let raw = match get_disk_cache_json(plenar_path).await {
            Some(raw) => raw,
            None => {
                let raw_plenar = self.get_gegenstand_info(plenar_path).await?;
                if let Err(e) = disk_caching(plenar_path, raw_plenar.to_string()).await {
                    log::warn!("could cache raw {plenar_path}: {e:?}")
                }
                raw_plenar
            }
        };

        Self::raw_plenar_into_plenar_info(raw)
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

    async fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo> {
        // let string = serde_json::to_string_pretty(&raw_plenar).unwrap();
        // println!("string {string}");

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

        let date = NaiveDateTime::parse_from_str(
            plenar_content_0["einlangen"].as_str().unwrap(),
            "%Y-%m-%dT%H:%M:%S",
        )
        .unwrap();

        let updated_at = plenar_content_0["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
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

        let mut documents = documents(&raw_documents).unwrap_or_default();

        if let Some(content_2) = raw_plenar["content"].as_array()?.get(2) {
            if let Some(raw_praesidium) = extract_praesidium_from_progress(&content_2["progress"]) {
                documents.extend(
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

        let absences = extract_absences(&documents).await;
        log::info!("!!!!!!! absences: {absences:?}");

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
            date,
            updated_at,
            gp: gp_code,
            absences,
        })
    }
}

fn extract_praesidium_from_progress(raw_progress: &Value) -> Option<&Value> {
    for step in raw_progress.as_array()? {
        if step["text"].as_str()?.to_lowercase().contains("präsidium") {
            return Some(step);
        }
    }
    None
}

fn extract_praesidium_documents(raw_praesidium: &Value) -> Option<Vec<Document>> {
    documents(raw_praesidium["protocol"]["data"]["links"].as_array()?)
}

async fn extract_absences(documents: &[Document]) -> Option<Vec<String>> {
    for document in documents {
        if document.ftype == FileType::HTML
            && document.title.to_lowercase().contains("stenographisches")
        {
            return extract_absences_from_document(document).await;
        }
    }
    None
}

async fn extract_absences_from_document(doc: &Document) -> Option<Vec<String>> {
    let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
    let text = extractor
        .extract_text_from_url(&doc.link, None, true)
        .await?;
    // println!("processing noew: absence doc: {doc:?}");
    extract_absences_from_text(&text)
}

fn extract_absences_from_text(text: &str) -> Option<Vec<String>> {
    // remove this later -> only for testing purposes
    // if text.len() < 50000 {
    //     return None;
    // }

    let text = &text.replace('\u{a0}', " ");
    let text = text.replace('\u{ad}', "");
    let titles = unique_titles();

    let text = text
        .split_inclusive(' ')
        .filter(|x| !titles.contains(x.trim_matches(char::is_whitespace)))
        .collect::<String>();

    let absence_marker_regex = Regex::new(r"verhindert\s*gemeldet").unwrap();
    let absence_match = absence_marker_regex.find(&text)?;

    let mut start_idx = absence_match.start();

    const STOP_CHARS: [u8; 4] = [b'.', b'!', b'\n', b'?'];

    // should handle these "Ich gebe bekannt, dass Frau Dr. Mertel, Frau Dr. Partik-Pablé, Frau Dr. Povysil, Herr Dr. Puttinger und Herr Ing. Maderthaner für die heutige Sitzung als verhindert gemeldet sind."
    while !STOP_CHARS.contains(&text.as_bytes()[start_idx]) {
        start_idx -= 1;
    }

    let open_ended_absence = &text[start_idx + 1..];
    // println!("open_ended_absence: {}", &open_ended_absence[..500]);

    // let print = &text[start_idx..start_idx + 400];
    // println!("print: {print}");

    // open_ended_absence.replace(&["Hi", "Bi"], " ");

    // println!("open_ended_absence: {:?}", &open_ended_absence[..1001]);
    let info = &open_ended_absence[..open_ended_absence
        .find(".\n")
        .unwrap_or(usize::MAX)
        .min(open_ended_absence.find(". \n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(".\r\n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(". \r\n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(".  \n").unwrap_or(usize::MAX))
        // need to find the specific absence entry (again)
        // ? /dokument/XXV/NRSITZ/88/fnameorig_501525.html
        .min(open_ended_absence.find(". (Abg").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(", die ich").unwrap_or(usize::MAX))
        .min(
            open_ended_absence
                .find(", Zuweisungen")
                .unwrap_or(usize::MAX),
        )
        .min(open_ended_absence.find(".").unwrap_or(usize::MAX))
        .min(
            open_ended_absence
                .find(". (Ruf bei der ÖVP: ... alle von der SPÖ!)")
                .unwrap_or(usize::MAX),
        )
        .min(
            open_ended_absence
                .find(". (s. auch S. 32)")
                .unwrap_or(open_ended_absence.len()),
        )];

    log::info!(
        "-------------
    info: {info:?}"
    );

    let skip_words = [
        "dass",
        "MMSc",
        "diplômé",
        "verhindert",
        "gemeldet",
        "gemeldet:",
        "für",
        "die",
        "heutige",
        "Sitzung",
        "ist",
        "niemand",
        "sind",
        "Mitglieder",
        "Hohen",
        "Hauses",
        "Abgeordnete:",
        "Abgeordnete,",
        "Abgeordneten",
        "Abgeordneten:",
        "heute",
        "Abgeordneter",
        "Abgeordnete",
        "Herr",
        "Nationalrates",
        "Präsident",
        "Präsidentin",
        "unsere",
        "leider",
        "erkrankte",
        "weiters",
        "auch",
        "Zweite",
        "heutigen",
        "Sitzungstag",
        "Sitzungen",
        "Frau",
        "gebe",
        "bekannt",
        "folgende",
        "diesem",
        "Sinne",
        "Pilz:",
        "doch",
        "nicht",
        "Kickl!",
        "keine",
        "Klubvorsitzende",
        "Klubvorsitzender",
        "weitere",
        "Zwischenrufe",
        "Zwischenruf",
        "(Rufe",
        "Heiterkeit",
        "wird",
        "immer",
        "automatisch",
        "entschuldigt!)",
        "Kollege",
        "herzlichen",
        "Dank!",
        "sollten",
        "darüber",
        "unterhalten",
        "warum",
        "fehlend",
        "eingetragen",
        "Grünen)",
        "insgesamt",
        "42",
        "nämlich",
        "ganze",
        "vertreten",
        "durch",
        "Vizekanzler",
        "entschuldigt",
        "damit",
        "Kollegen",
        "Kolleginnen",
        "einige",
        "nämlich",
        "Dipl",
        "heute",
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    let info = info
        .split(',')
        .flat_map(|x| x.split("und"))
        .flat_map(|x| x.split("sowie"))
        .map(|x| {
            x.split_inclusive(char::is_whitespace).filter(|split| {
                !(split.chars().filter(|x| x.is_alphabetic()).count() <= 3
                    || split.contains('.')
                    || {
                        let mut ret = false;
                        if skip_words.contains(split.trim_matches(char::is_whitespace)) {
                            ret = true;
                        }
                        ret
                    })
            })
        })
        .map(|x| x.collect::<String>())
        .collect::<Vec<_>>();

    Some(
        filter_name_extraction(info.iter().map(|x| x.as_str()))
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>(),
    )
}

fn extract_call_to_order_info_from_stages(
    ityp: &str,
    inr: u64,
    stages: &Value,
) -> Vec<CallToOrderInfo> {
    let mut call_to_orders = vec![];
    for (person_path, person_name, call_to_order_path) in
        extract_call_to_order_paths_from_stages(stages).unwrap_or_default()
    {
        let receiver_id = person_url_to_id(&person_path).unwrap();
        let (call_to_order_inr, call_to_order_ityp) =
            extract_call_to_order_id_from_path(&call_to_order_path);

        call_to_orders.push(CallToOrderInfo {
            description: "".into(),
            ityp: call_to_order_ityp.unwrap_or("GO").into(),
            inr: call_to_order_inr.unwrap().parse().unwrap(),
            receiver: person_name,
            receiver_id,
            links_to: Some(format!("{ityp}/{inr}")),
        });
    }
    call_to_orders
}

// #[macro_export]
// macro_rules! key_from_speeches {
//     ($speeches:ident) => {
//         $speeches
//             .iter()
//             .map(|speech| speech.delegate_id)
//             .collect::<Vec<u64>>()
//     };
// }

fn combine_speech_info(
    debates_with_speeches: Vec<(DebateInfo, Vec<PlenarSpeech>)>,
    document_link_speeches: Vec<Vec<Speech>>,
) -> Vec<Debate> {
    let document_link_speeches = document_link_speeches
        .into_iter()
        .map(|speeches| {
            (
                speeches
                    .iter()
                    .map(|speech| speech.delegate_id)
                    .collect::<Vec<_>>(),
                speeches,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let debates_with_speeches_keys = debates_with_speeches
        .into_iter()
        .map(|(about, speeches)| {
            (
                speeches
                    .iter()
                    .map(|speech| speech.speech.delegate_id)
                    .collect::<Vec<_>>(),
                about,
                speeches,
            )
        })
        .collect::<Vec<_>>();

    let mut debates = vec![];
    for (debates_key, debate_info, mut debate_speeches) in debates_with_speeches_keys {
        let document_link_speeches = document_link_speeches
            .get(&debates_key)
            .map(|speeches| speeches.as_slice())
            .unwrap_or_default();
        if document_link_speeches.is_empty() {
            log::warn!(
                "Keine document links! Aber {} speeches",
                debate_speeches.len()
            );
        }
        for (debate_speech, document_link_speech) in
            debate_speeches.iter_mut().zip(document_link_speeches)
        {
            debate_speech.speech = document_link_speech.clone();
        }
        debates.push(Debate {
            info: debate_info,
            speeches: debate_speeches,
        });
    }
    debates
}

pub fn extract_speeches(past_debates: &[Value]) -> Vec<(DebateInfo, Vec<PlenarSpeech>)> {
    past_debates
        .iter()
        .flat_map(|past_debate| {
            Some((
                past_debate["text"].as_str()?,
                past_debate["speeches"].as_array()?,
                past_debate["agenda"].as_array()?,
            ))
        })
        .filter(|(_, speeches, _)| !speeches.is_empty())
        .flat_map(|(text, speeches, refs)| {
            let references = refs
                .iter()
                .flat_map(|x| {
                    Some(Reference {
                        gp: x["gp_code"].as_str()?.to_string(),
                        ityp: x["ityp"].as_str()?.to_string(),
                        inr: x["inr"].as_i64()? as u32,
                    })
                })
                .collect::<Vec<_>>();
            Some((
                DebateInfo {
                    about: text.to_string(),
                    references,
                },
                speeches
                    .iter()
                    .flat_map(|speech| {
                        let delegate_id = speech[3].as_u64()?;
                        let ty = speech[5].as_str()?.to_string();
                        let duration = speech[7].as_str()?.split(':').collect::<Vec<&str>>();
                        let duration_in_seconds = duration[0].parse::<u32>().ok()? * 60
                            + duration[1].parse::<u32>().ok()?;

                        Some(PlenarSpeech {
                            // TODO: get spoken text html urls, use content[0]
                            speech: Speech {
                                delegate_id,
                                delegate_display_name: String::new(),
                                opinion: Opinion::None,
                                spoken_text_html_urls: vec![],
                            },
                            ty,
                            duration_in_seconds,
                        })
                    })
                    .collect::<Vec<_>>(),
            ))
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, iter};

    use common_scrapes::{Document, Opinion, Speech, SpeechTextInfo};

    use crate::{plenar::extract_absences_from_document, AustriaScraper, Scraper, TextExtractor};

    use super::Plenar;

    #[tokio::test]
    async fn test_get_gegenstand_paths_plenar() {
        let scraper = AustriaScraper::default();
        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        println!("paths: {paths:?}");
    }

    #[tokio::test]
    async fn test_combine_warning() {
        let mut scraper = AustriaScraper::default();
        simple_logger::init_with_level(log::Level::Warn).unwrap();

        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                println!("path: {}{path}", AustriaScraper::BASE_URL);
                let _plenar_info = scraper.get_plenar(&path).await.unwrap();
                // plenar_info
            }
        }
    }

    #[tokio::test]
    async fn test_all_speeches_extract() {
        let mut scraper = AustriaScraper::default();

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut all_speeches_count = 0;
        let mut i = 0;
        'outer: for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                i += 1;
                println!("path: {path}");
                // path with no content at 1: /gegenstand/XXVII/NRSITZ/216

                // newer ones do not provide a document

                let plenar_info = scraper.get_plenar(&path).await.unwrap();
                println!(
                    "{:?}, {}, {:?}",
                    plenar_info.inr, plenar_info.gp, plenar_info.date
                );

                if i == 2 {
                    // break 'outer;
                }
                for debate in plenar_info.clone().debates {
                    for speech in debate.speeches {
                        if let Some(spoken_text_url) = speech.speech.spoken_text_html_urls.first() {
                            let text = extractor
                                .speech_extraction_with_fit(
                                    spoken_text_url,
                                    &speech.speech.delegate_display_name,
                                )
                                .await
                                .unwrap();

                            if speech.speech.delegate_id == 5649 {
                                if text.len() > 10000 {
                                    println!("text: {text:?}");
                                    println!("plenar_info: {plenar_info:?}");
                                    println!();
                                    println!();
                                    println!();
                                    println!();
                                    println!();
                                    println!("spoken_text_url: {spoken_text_url:?}");
                                }
                            }
                            all_speeches_count += 1;
                        }
                    }
                }
            }
        }

        // 20805
        // println!("speeches: {all_speeches_count}");
    }

    #[tokio::test]
    async fn test_look_all_plenar_gegenstand_paths() {
        let mut scraper = AustriaScraper::default();

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut all_speeches_count = 0;
        let mut i = 0;
        'outer: for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                i += 1;
                println!("path: {path}");
                // path with no content at 1: /gegenstand/XXVII/NRSITZ/216

                // newer ones do not provide a document

                let plenar_info = scraper.get_plenar(&path).await.unwrap();
                println!(
                    "{:?}, {}, {:?}",
                    plenar_info.inr, plenar_info.gp, plenar_info.date
                );

                if i == 2 {
                    // break 'outer;
                }
                for debate in plenar_info.debates {
                    for speech in debate.speeches {
                        if let Some(spoken_text_url) = speech.speech.spoken_text_html_urls.first() {
                            extractor
                                .speech_extraction_with_fit(
                                    spoken_text_url,
                                    &speech.speech.delegate_display_name,
                                )
                                .await
                                .unwrap();
                            all_speeches_count += 1;
                        }
                    }
                }
                if all_speeches_count % 100 == 0 {
                    // dbg!("all s c: {all_speeches_count}");
                    // break 'outer;
                }
                // let speeches_count: usize = plenar_info.debates.iter().map(|d| d.speeches.len()).sum();
                // all_speeches_count += speeches_count;
                // println!("plenar_info: {:?}", plenar_info.absences);
            }
        }

        // 20805
        // println!("speeches: {all_speeches_count}");
    }

    #[tokio::test]
    async fn test_absences_for_xxvii_247() {
        let scraper = AustriaScraper::default();
        let plenar_info = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/247")
            .await
            .unwrap();

        assert_eq!(
            [
                "Martin Engelberg",
                "Reinhold Lopatka",
                "Petra Bayr",
                "Julia Elisabeth",
                "Klaus Köchl",
                "Michael Seemayer",
                "Petra Tanzler",
                "Hubert Fuchs",
                "Herbert Kickl",
                "Christian Ragger",
                "Wolfgang Zanger",
                "Ernst-Dziedzic",
                "Heike Grebien",
                "Bedrana Ribo",
                "Karin Doppelbauer",
                "Fiona Fiedler",
                "Julia Seidl"
            ],
            *plenar_info.absences.unwrap().clone()
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_85() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/85")
            .await
            .unwrap();

        println!("plenar: {plenar:?}");
        assert_eq!(
            plenar
                .absences
                .unwrap()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Andreas Kollross",
                "Petra Vorderwinkler",
                "Christian Lausch",
                "Henrike Brandstötter"
            ]
        )
    }
    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_19() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/19")
            .await
            .unwrap();

        println!("plenar: {plenar:?}");
        assert_eq!(
            plenar
                .absences
                .unwrap()
                .iter()
                .map(|x| x.as_str())
                .collect::<Vec<_>>(),
            vec![
                "Hermann Gahr",
                "Kira Grünberg",
                "Josef Hechenberger",
                "Franz Hörl",
                "Rebecca Kirchbaumer",
                "Andreas Minnich",
                "Josef Moser",
                "Elisabeth Pfurtscheller",
                "Gabriela Schwarz",
                "Johann Singer",
                "Alexandra Tanda"
            ]
        );
    }

    // /gegenstand/XXVIII/NRSITZ/6
    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxviii_nrsitz_5_absences() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/5")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences,
            Some(vec![
                "Giuliani-Sterrer".to_string(),
                "Herbert".to_string(),
                "Gmeinbauer".to_string(),
                "Voglauer".to_string(),
                "Zadić".to_string()
            ])
        );
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxviii_nrsitz_6_absences() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/6")
            .await
            .unwrap();

        dbg!(&plenar);
        assert_eq!(
            plenar.absences,
            Some(vec![
                "Marie-Christine Giuliani-Sterrer".to_string(),
                "Werner Herbert".to_string(),
                "Daniela Gmeinbauer".to_string(),
                "Olga Voglauer".to_string(),
                "Alma Zadić".to_string()
            ])
        );
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_211() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/211")
            .await
            .unwrap();

        assert_eq!(plenar.gp, "XXVII");
        assert_eq!(plenar.ityp, "NRSITZ");
        assert_eq!(plenar.inr, 211);
        assert_eq!(plenar.debates[0].info.about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        assert_eq!(
            plenar.debates[0].speeches[0].speech.spoken_text_html_urls[0].document_url,
            "/dokument/XXVII/NRSITZ/211/A_-_12_03_00_00295752.html"
        );
        assert_eq!(plenar.debates[0].speeches[0].duration_in_seconds, 831);
        assert_eq!(
            plenar.debates[1].speeches[plenar.debates[1].speeches.len() - 4].duration_in_seconds,
            300
        );
        assert_eq!(
            plenar.debates[1].speeches[plenar.debates[1].speeches.len() - 4]
                .speech
                .spoken_text_html_urls[0]
                .document_url,
            "/dokument/XXVII/NRSITZ/211/A_-_15_24_25_00295995.html"
        );

        let cto = &plenar.call_to_orders[0];
        assert_eq!(cto.inr, 1239);
        assert_eq!(cto.ityp, "GO");
        assert_eq!(cto.links_to, Some("NRSITZ/211".into()));
        assert_eq!(cto.receiver_id, 83101);
        assert_eq!(cto.receiver, "Sigrid Maurer, BA");

        let cto = &plenar.call_to_orders[1];
        assert_eq!(cto.inr, 1240);
        assert_eq!(cto.ityp, "GO");
        assert_eq!(cto.links_to, Some("NRSITZ/211".into()));
        assert_eq!(cto.receiver_id, 83101);
        assert_eq!(cto.receiver, "Sigrid Maurer, BA");

        let cto = &plenar.call_to_orders[3];
        assert_eq!(cto.inr, 1238);
        assert_eq!(cto.ityp, "GO");
        assert_eq!(cto.links_to, Some("NRSITZ/211".into()));
        assert_eq!(cto.receiver_id, 67199);
        assert_eq!(cto.receiver, "Mag. Wolfgang Gerstl");
    }

    #[tokio::test]
    async fn test_get_all_plenar_things_but_with_speech_fit_extraction() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut scraper = AustriaScraper::default();

        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                let plenar = scraper.get_plenar(&path).await.unwrap();
                let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

                for debate in &plenar.debates {
                    for speech in &debate.speeches {
                        if speech.speech.spoken_text_html_urls.first().is_none() {
                            continue;
                        }
                        let speech_info = speech.speech.spoken_text_html_urls.first().unwrap();
                        let speech_text = extractor
                            .speech_extraction_with_fit(
                                speech_info,
                                &speech.speech.delegate_display_name,
                            )
                            .await
                            .unwrap();

                        if speech_text.len() <= 130 {
                            println!(
                                "-------------------------------------- {}, {}{}",
                                speech.speech.delegate_display_name,
                                AustriaScraper::BASE_URL,
                                speech_info.document_url
                            );
                            println!("speech_text: {speech_text}");
                            println!("--------------------------------------");
                        }
                    }
                }
            }
        }
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxii_nrsitz_117() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXII/NRSITZ/117")
            .await
            .unwrap();
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut lengths = vec![];
        let mut required_lengths = [
            14849, 12113, 9967, 12443, 9509, 11478, 10661, 9885, 9146, 12137, 6351, 7442, 5794,
            7882, 10698, 6696, 8055, 6606, 3952, 4820, 2847, 4475, 4653, 3411, 6138, 6327, 2633,
            7536, 3480, 3287, 3128, 6117, 7095, 8463, 7306, 1015, 8520, 6135, 7353, 5449, 2367,
            7333, 5780, 5496, 6049, 5612, 4539, 2275, 3605, 4296, 2371, 3379, 3472, 10134, 5185,
            7992, 3920, 2025, 3330, 6944, 2324, 3966, 2189, 3852, 4305, 2931, 4050, 2356, 3615,
            3031, 2246, 3016, 2717, 1408, 2049, 1989, 2416, 1823, 1803, 2173, 2071, 1978, 1657,
            5448, 2092, 4084, 1604, 2214, 1940, 4432, 2082, 3262, 3519, 2385, 2468, 2442,
        ]
        .iter();
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                let speech_info = speech.speech.spoken_text_html_urls.first().unwrap();
                let speech_text = extractor
                    .speech_extraction_with_fit(speech_info, &speech.speech.delegate_display_name)
                    .await
                    .unwrap();
                lengths.push(speech_text.len());
                let required_length = required_lengths.next().unwrap();
                if (speech_text.len() as f32 / *required_length as f32 - 1.).abs() >= 0.04 {
                    println!(
                        "{} -------------------------------------- {}{}",
                        &speech.speech.delegate_display_name,
                        AustriaScraper::BASE_URL,
                        speech_info.document_url
                    );
                    println!("speech_text: {speech_text}");
                    println!("--------------------------------------");
                    panic!("{} required: {}", speech_text.len(), required_length)
                }
                // println!(
                //     "-------------------------------------- {}, {}{}",
                //     speech.speech.delegate_display_name,
                //     AustriaScraper::BASE_URL,
                //     speech_info.document_url
                // );
                // println!("speech_text: {speech_text}");
                // println!("--------------------------------------");
            }
        }

        // println!("lengths: {lengths:?}");
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_117() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();

        // assert_eq!(plenar.debates[1].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        let first_speech = &plenar.debates[0].speeches[0];
        assert_eq!(first_speech.speech.delegate_id, 5642);
        assert_eq!(first_speech.speech.opinion, Opinion::Against);

        let opinion_none_speech = &plenar.debates[0].speeches[2];
        assert_eq!(opinion_none_speech.speech.delegate_id, 83127);
        assert_eq!(opinion_none_speech.speech.opinion, Opinion::Against);
        assert_eq!(opinion_none_speech.duration_in_seconds, 5 * 60 + 12);

        let next_debate_speech = &plenar.debates[1].speeches[1];
        assert_eq!(next_debate_speech.speech.delegate_id, 2006);
        assert_eq!(next_debate_speech.speech.opinion, Opinion::Infavor);
        assert_eq!(next_debate_speech.duration_in_seconds, 5 * 60 + 32);

        let last_speech_of_next_debate = plenar.debates[1].speeches.last().unwrap();
        assert_eq!(last_speech_of_next_debate.speech.delegate_id, 83299);
        assert_eq!(last_speech_of_next_debate.speech.opinion, Opinion::Infavor);
        assert_eq!(last_speech_of_next_debate.duration_in_seconds, 2 * 60 + 20);

        let next_debate_speech = &plenar.debates[2].speeches[0];
        assert_eq!(next_debate_speech.speech.delegate_id, 8178);
        assert_eq!(next_debate_speech.speech.opinion, Opinion::Against);
        assert_eq!(next_debate_speech.duration_in_seconds, 4 * 60 + 28);

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut lengths = vec![];
        let mut required_lenghts = [
            10832, 5661, 3878, 8295, 11226, 9298, 8087, 3890, 4564, 4038, 4950, 4216, 3592, 2600,
            2739, 1902, 2150, 6368, 4873, 4830, 5002, 6103, 9346, 4242, 7348, 2641, 3348, 2743,
            2917, 3964, 440, 3532, 2464, 3945, 2896, 6627, 5222, 4424, 2266, 2860, 3043, 2953,
            4623, 3139, 4498, 2449, 1973, 1600, 5548, 2939, 2135, 1875, 4010, 3828, 18815, 5729,
            13164, 7177, 4682, 4641, 6584, 5366, 5089, 3362, 3956, 5698, 6243, 4292, 5468, 4922,
            6495, 5364, 4746, 8129, 4569, 5283, 4702, 7174, 3447, 4147, 4492, 5104, 1315, 4355,
            5224, 3165, 3388, 4848, 1536, 2517, 3096, 2989, 2428, 3643, 3599, 1893, 2878, 2712,
            2562, 5737, 6611, 3943, 4050, 1812, 1960, 3379, 2407, 3243, 3065, 3699, 3102, 6188,
            4963, 2536, 1856, 6821, 7616, 3209, 3915, 2276, 2855, 4114, 3718, 4115, 3974, 2238,
            1284, 3041, 1661, 4066, 3815, 2966, 3395, 2212, 2464, 4087, 3612, 3172, 2496, 4265,
            4285, 3061, 2578, 3222, 3161, 1860, 2780, 3097, 3123, 2990, 3324, 3200, 255, 5484,
            3103, 2784, 2724, 4508, 2985, 1193, 1984, 8285, 7088, 4518, 5759, 3436, 2327, 2138,
            5425, 1950, 1265, 1420, 1068,
        ]
        .iter();
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                dbg!(speech);
                let speech_info = speech.speech.spoken_text_html_urls.first().unwrap();
                let speech_text = extractor
                    .speech_extraction_with_fit(speech_info, &speech.speech.delegate_display_name)
                    .await
                    .unwrap();
                lengths.push(speech_text.len());
                let required_length = required_lenghts.next().unwrap();
                if (speech_text.len() as f32 / *required_length as f32 - 1.).abs() >= 0.04 {
                    println!(
                        "{} -------------------------------------- {}{}",
                        &speech.speech.delegate_display_name,
                        AustriaScraper::BASE_URL,
                        speech_info.document_url
                    );
                    println!("speech_text: {speech_text}");
                    println!("--------------------------------------");
                    panic!("{} required: {}", speech_text.len(), required_length)
                }
                // println!(
                //     "-------------------------------------- {}{}",
                //     AustriaScraper::BASE_URL,
                //     speech_info.document_url
                // );
                // println!("speech_text: {speech_text}");
                // println!("--------------------------------------");
            }
        }

        // assert_eq!(&required_lenghts, lengths.as_slice());
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xx() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XX".into()];

        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");

        for path in paths {
            println!("path: {path} ---");
            let plenar = scraper.get_plenar(&path).await.unwrap();

            let Some(debates) = plenar.debates.first() else {
                println!("empty speeches");
                continue;
            };

            if debates.speeches.is_empty() {
                println!("empty speeches")
            }
        }
    }

    #[tokio::test]
    async fn test_plenar_speech_and_legis_init_speech_mapping() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();
        for debate in plenar.debates {
            for speech in debate.speeches {
                if speech.speech.delegate_id != 72959 {
                    continue;
                }
                // let text = extractor.extract_text_from_url(&speech).await;
                // println!("{}", text);
            }
        }
    }

    #[tokio::test]
    async fn test_duplicate_plenar_speeches() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXIV".to_string()];
        let plenar = scraper
            .get_plenar("/gegenstand/XXIV/NRSITZ/150")
            .await
            .unwrap();
        let mut idx = 0;
        for debate in plenar.debates {
            for speech in debate.speeches {
                if speech.speech.delegate_id != 22573 {
                    continue;
                }

                if idx == 1 {
                    println!("speech: {speech:?}");
                    let text = extractor
                        .speech_extraction_with_fit(
                            &speech.speech.spoken_text_html_urls[0],
                            &speech.speech.delegate_display_name,
                        )
                        .await;
                    println!("{:?}", text);
                }

                idx += 1;
                // println!("speech: {speech:?}");
                // println!("_----------")
            }
        }
    }

    #[tokio::test]
    async fn test_absences_extraction_from_doc() {
        let link = "/dokument/XXIII/NRSITZ/72/fnameorig_144783.html".to_string();
        let doc = Document {
            title: "nix".into(),
            link,
            ftype: common_scrapes::FileType::HTML,
        };

        let absences = extract_absences_from_document(&doc).await.unwrap();
        println!("absences: {absences:?}");
        assert_eq!(&absences, &Vec::<String>::new());
    }

    //
    #[tokio::test]
    async fn test_absences_extraction_from_doc2() {
        let link = "/dokument/XXIV/NRSITZ/1/fnameorig_144815.html".to_string();
        let doc = Document {
            title: "nix".into(),
            link,
            ftype: common_scrapes::FileType::HTML,
        };

        let absences = extract_absences_from_document(&doc).await.unwrap();
        assert_eq!(absences, vec!["Wilhelm Molterer".to_string()]);
        println!("absences: {absences:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxvii_189() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/189")
            .await
            .unwrap();
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxvii_243() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/243")
            .await
            .unwrap();
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxiv_55() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXIV/NRSITZ/55")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                let speech_text = text_extractor
                    .speech_extraction_with_fit(
                        &speech.speech.spoken_text_html_urls[0],
                        &speech.speech.delegate_display_name,
                    )
                    .await
                    .unwrap();
                println!("speech_text: {speech_text:?}");
            }
        }
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxv_55() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXV/NRSITZ/55")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                dbg!(speech);
                let speech_text = text_extractor
                    .speech_extraction_with_fit(
                        &speech.speech.spoken_text_html_urls[0],
                        &speech.speech.delegate_display_name,
                    )
                    .await
                    .unwrap();
                println!("speech_text: {speech_text:?}");
            }
        }
        println!("plenar: {plenar:?}");
    }



    #[tokio::test]
    async fn test_plenar_xxviii_35() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/35")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                dbg!(speech);
                let speech_text = text_extractor
                    .speech_extraction_with_fit(
                        &speech.speech.spoken_text_html_urls[0],
                        &speech.speech.delegate_display_name,
                    )
                    .await
                    .unwrap();
                println!("speech_text: {speech_text:?}");
            }
        }
        // println!("plenar: {plenar:?}");
    }

    /*

    */

    #[tokio::test]
    async fn test_plenar_speech_diesner_martina() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        let speech = Speech {
            delegate_id: 16218,
            delegate_display_name: "Martina Diesner-Wais (V)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: None,
                site_end: None,
                document_url: "/dokument/XXVIII/NRSITZ/3/fnameorig_1688660.html#86".into(),
            }],
        };

        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap_or_default();
        println!("speech_text: {speech_text}");
        assert_eq!(speech_text, "Martina Diesner-Wais (ÖVP): Sehr geehrte Frau Präsidentin! Liebe Abgeordnete! Liebe Zuseher hier im Parlament und auch vor den Fernsehgeräten! Ich möchte gleich zu Beginn im Namen meiner Kollegin Bettina Zopf den Seniorenbund von Neukirchen begrüßen. – Willkommen im Parlament! (Beifall bei der ÖVP sowie bei Abgeordneten von FPÖ und SPÖ. – Präsident Rosenkranz übernimmt den Vorsitz.)
Meine Damen und Herren, wir befassen uns heute mit der Nachnominierung und der Wahl eines neuen Mitglieds der Volksanwaltschaft. Diese Nachnominierung ist notwendig geworden, weil Herr Dr. Rosenkranz, der bisher Volksanwalt war, in das Nationalratspräsidium gewechselt ist.
Unsere Volksanwaltschaft ist ein unabhängiges Kontrollorgan und hat eine tragende Rolle in der demokratischen Kultur in unserem Land. Die Volksanwaltschaft ist Ansprechpartner für Bürgerinnen und Bürger, die Missstände in der Verwaltung sehen und aufzeigen wollen, und sorgt dafür, dass die Verwaltung auch weiterhin fair, bürgernah und rechtskonform bleibt.
Die Volksanwaltschaft ist niederschwellig erreichbar. Sie können sie entweder an Sprechtagen telefonisch, über das Internet oder natürlich auch persönlich erreichen. Sie ist eine Institution, die dazu beiträgt, das Vertrauen der Bevölkerung in unsere staatlichen Einrichtungen zu stärken. 
Die Volksanwaltschaft ist aber auch zum Schutz der Menschenrechte da und wird dahin gehend auch immer wieder tätig. Die Aufgabenbereiche, die Dr. Walter Rosenkranz in der Volksanwaltschaft innehatte, wird nun unsere neue Volksanwältin übernehmen, und dies sind wichtige Themenbereiche.
Sie umfassen Polizei, Fremden- und Asylrecht, Denkmalschutz, Land-, Forst- und Wasserwirtschaft, Natur- und Umweltschutz, Gewerbe und Betriebsanlagen, Kindergärten, Schulen, Universitäten, Verkehr, Gemeindeabgaben und auch Agrarangelegenheiten. Das ist eine große Bandbreite, und daher ist es eine anspruchsvolle und wirklich auch verantwortungsvolle Funktion.
Die Volksanwaltschaft ist ein Organ, der verlängerte Arm unseres Parlaments, und durch die Berichte der Volksanwaltschaft konnten wir hier auch schon viele Verbesserungen erzielen. Wir haben in Österreich einen verfassungsrechtlichen Bestellmodus, der besagt, dass die drei stärksten Parteien die Vorschläge für die Volksanwälte bringen können, und das gibt nun der FPÖ die Möglichkeit, Frau MMag. Elisabeth Schwetz vorzuschlagen. 
Wir als ÖVP werden diesem Vorschlag zustimmen. Ich als Vorsitzende des Volksanwaltschaftsausschusses freue mich schon auf eine gute Zusammenarbeit, denn es ist wichtig, dass die Volksanwaltschaft und das Parlament gut zusammenarbeiten – im Sinne unserer Bürger und Bürgerinnen. (Beifall bei der ÖVP.)
14.59");
    }
    #[tokio::test]
    async fn test_plenar_speech_paul_stich() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        let speech = Speech {
            delegate_id: 30707,
            delegate_display_name: "Paul Stich (S)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: None,
                site_end: None,
                document_url: "/dokument/XXVIII/NRSITZ/35/A_-_23_40_42_00818253.html#211".into(),
            }],
        };

        dbg!(&speech);
        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap_or_default();
        println!("speech_text: {speech_text}");
        assert_eq!(speech_text, "Paul Stich (SPÖ): Vielen Dank, Herr Präsident! Sehr geehrte Frau Ministerin! Liebe Kolleginnen und Kollegen! Liebe Zuseherinnen und Zuseher, sofern Sie zu dieser späten Stunde noch auf sind! Die geopolitischen Konflikte auf der Welt spitzen sich zu, nicht zuletzt durch neu oder wieder aufs politische Parkett kommende scheinbare Kriegstreiber wie Donald Trump. Das zeigt sich in Debatten hier im Haus, das zeigt sich auch im Landesverteidigungsbericht, und das ergibt einerseits ganz viel an militärstrategischer Denkaufgabe und auf der anderen Seite aber auch vielfältige Aufgaben für das österreichische Bundesheer. 
Wenn wir über die geopolitischen Ereignisse sprechen, gilt es aus unserer Sicht vor allem eines herauszustreichen: dass es Krieg immer und überall mit allen Mitteln zu verhindern gilt. Es ist traurig, dass wir in einer Welt leben, in der diese Einschätzung nicht von allen und nicht überall geteilt wird, denn für manche scheint Krieg vielmehr wie eine Art Schachspiel zu sein. Die sitzen dann vor dem PC auf Twitter, meistens in ruhigen, recht gut geschützten Räumen, und diskutieren darüber, ob man nicht vielleicht doch irgendwo ein paar Leute opfern soll, quasi wie einen Bauern beim Schach irgendwo vorziehen sollte, um kurzfristig Menschen zu opfern, um langfristig daraus einen strategischen Vorteil zu ziehen. Ich glaube, gerade in diesem Zusammenhang ist es wichtig, da auch einen sehr bewussten Kontrastpunkt zu setzen, denn Krieg bringt Leid, Tod und Zerstörung, bringt Wunden in Familien, die über Generationen da sind, und wer beginnt, tote Zivilist:innen aufzurechnen, um daraus Legitimation für weitere Angriffe oder Tote zu schaffen, der wird menschlich am Ende des Tages immer verlieren. (Beifall bei der SPÖ.)
Denn: Wie fühlt sich Krieg für diejenigen an, die ihn hautnah erleben? – Die meisten von uns, und zum Glück auch ich, können diese Frage nicht persönlich beantworten. Deshalb will ich mich der Worte der persönlichen Erinnerung von Almasa Salihović bedienen, einer Überlebenden des Massakers von Srebrenica und Mitarbeiterin im Srebrenica Memorial Center, die sie gestern im Rahmen einer sehr würdigen Gedenkveranstaltung hier im Parlament hat ausführen können. Sie sagt ganz konkret über die Schrecken des Krieges – ich zitiere –: Meine Mutter, meine Schwester und ich mussten mit ansehen, wie Männer und Jungen von den Soldaten getrennt wurden. Ich erinnere mich an das Schreien, an das Flehen, an die Hilflosigkeit. Mein Bruder Abdullah wurde nie wieder gesehen. Er war erst 18 Jahre alt. Er hatte versucht, mit meiner Schwester die Basis zu verlassen, wurde aber festgenommen. Aus ihrem Bericht weiß ich, sie sahen sich noch einmal an, bevor er abgeführt wurde. Sie wusste es, er wusste es, und nun weiß auch ich es: Das war ihr letzter Abschied. – Zitatende.
Diese durchwegs berührenden Worte sind für uns aus meiner Sicht Handlungsauftrag auf zwei Ebenen: zum einen, weil das österreichische Bundesheer – und hier schließt sich zu einem gewissen Grad der Kreis – seit Jahrzehnten ja auch Friedenstruppen in Bosnien stellt, um den Frieden von 1995 abzusichern, was ein sehr gutes Beispiel dafür ist, wie man ganz besonders auch als neutraler Staat einen wichtigen Beitrag in der internationalen Gemeinschaft leistet, ganz besonders in einer Situation, in der sich auch in Bosnien innenpolitisch die Lage durchwegs zuspitzt; und zum anderen, weil es natürlich auch ein politischer Auftrag ist, der untrennbar mit der militärischen Debatte, die wir hier führen, verbunden ist, nämlich dass wir sehen, dass die Tendenz immer mehr wieder dahin geht, dass Krieg und außenpolitisch-militärische Schläge zu einem tolerierten Mittel der globalen Außenpolitik werden. Diese Entwicklung ist brandgefährlich, und wir werden im Rahmen dieser Plenartage auch noch über die Kandidatur Österreichs für den UN-Sicherheitsrat sprechen. Lassen Sie mich nur so viel vorgreifen: Ich halte das für einen guten, richtigen und wichtigen Schritt, damit die Stimmen der Deeskalation und der friedlichen Koexistenz ganz besonders im Sinne der österreichischen Neutralitätspolitik auch international wieder gestärkt werden (Beifall bei der SPÖ), damit Soldaten dieser Welt – egal ob im österreichischen Bundesheer oder in allen anderen Heeren – die Schlachtfelder weiterhin nur aus den Geschichtsbüchern kennen. – Vielen Dank. (Beifall bei der SPÖ und bei Abgeordneten der ÖVP.)
23.45")
    }
    // println!("plenar: {plenar:?}");

    #[tokio::test]
    async fn test_plenar_xxviii_3() {
        // let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/3")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                if speech.speech.spoken_text_html_urls.is_empty() {
                    continue;
                }
                dbg!(speech);
                let speech_text = text_extractor
                    .speech_extraction_with_fit(
                        &speech.speech.spoken_text_html_urls[0],
                        &speech.speech.delegate_display_name,
                    )
                    .await
                    .unwrap();
                println!("speech_text: {speech_text:?}");
            }
        }
        // println!("plenar: {plenar:?}");
    }


    #[tokio::test]
    async fn test_get_all_attributes() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut scraper = AustriaScraper::default();

        let mut attributes = HashSet::new();

        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            let gp = "XXII";
            println!("gp: {gp}");
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");


            for path in paths {
                let plenar = scraper.get_plenar(&path).await.unwrap();
                for debate in &plenar.debates {
                    for speech in &debate.speeches {
                        if speech.speech.spoken_text_html_urls.first().is_none() {
                            continue;
                        }
                        let first = &speech.speech.spoken_text_html_urls[0];
                        if first.document_url.contains('#') {
                            let attr = first.document_url.split('#').last().unwrap();
                            attributes.insert(attr.to_string());
                        }
                    }
                }
            }

            println!("attributes: {attributes:?}");
        }
    }
}
