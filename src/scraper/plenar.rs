use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, Utc};
use common_scrapes::{Document, PlenarSpeech, Reference};
use serde_json::{json, Value};

mod absences;
mod call_to_orders;
mod speeches;

use crate::{
    caching::{disk_caching, get_disk_cache_json},
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
    pub inr: u64,
    pub debates: Vec<Debate>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub call_to_orders: Vec<CallToOrderInfo>,
    pub gp: String,
    pub absences: Option<Absences>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Absences {
    pub absences: Vec<String>,
    pub source_document: Document,
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
        let updated_at = updated_at
            .map(|updated_at| {
                convert_vienna_datetime_to_utc(
                    NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap(),
                )
            })
            .flatten();

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
        let documents = documents(&raw_documents, AustriaScraper::BASE_URL).unwrap_or_default();
        let absences_documents = praesidium_documents
            .iter()
            .chain(&documents)
            .cloned()
            .collect::<Vec<_>>();

        let absences = extract_absences(&absences_documents).await;
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
            created_at,
            updated_at,
            gp: gp_code,
            absences,
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

#[cfg(test)]
mod tests {
    use std::{collections::HashSet, iter};

    use common_scrapes::{Document, Opinion, Speech, SpeechTextInfo};

    use crate::{text_utils::compare_words, AustriaScraper, Scraper, TextExtractor};

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
        return;
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
        return;
        let mut scraper = AustriaScraper::default();

        let mut extractor = TextExtractor::new();

        unsafe { std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false") };
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
                    plenar_info.inr, plenar_info.gp, plenar_info.created_at
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
    async fn test_look_all_plenar_speech_delegate_display_names() {
        return;
        let mut scraper = AustriaScraper::default();
        unsafe { std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false") };
        let mut unique_display_names = HashSet::new();
        'outer: for gp in [
            "XXVIII", "XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX",
        ] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                println!("path: {path}");

                let plenar_info = scraper.get_plenar(&path).await.unwrap();
                println!(
                    "{:?}, {}, {:?}",
                    plenar_info.inr, plenar_info.gp, plenar_info.created_at
                );

                for debate in plenar_info.debates {
                    for speech in debate.speeches {
                        println!("{}", speech.speech.delegate_display_name);
                        unique_display_names.insert(speech.speech.delegate_display_name);
                    }
                }
            }
        }
        let text = serde_json::to_string(&unique_display_names).unwrap();
        tokio::fs::write("unique_display_names.json", text)
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_look_all_plenar_gegenstand_paths() {
        return;
        let mut scraper = AustriaScraper::default();

        let mut extractor = TextExtractor::new();

        unsafe { std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false") };
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
                    plenar_info.inr, plenar_info.gp, plenar_info.created_at
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
            &[
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
            ]
            .as_slice(),
            &plenar_info.absences.unwrap().clone().absences
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
                .clone()
                .absences
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
                .absences
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
            plenar.absences.clone().unwrap().absences,
            vec![
                "Giuliani-Sterrer".to_string(),
                "Herbert".to_string(),
                "Gmeinbauer".to_string(),
                "Voglauer".to_string(),
                "Zadić".to_string()
            ]
        );
        println!("{:?}", plenar.absences);
        assert_eq!(
            plenar.absences.unwrap().source_document.link,
            "https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/5/A_-_09_09_06_00597342.html"
        );
        // println!("plenar: {plenar:?}");
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
            plenar.absences.clone().unwrap().absences,
            vec![
                "Marie-Christine Giuliani-Sterrer".to_string(),
                "Werner Herbert".to_string(),
                "Daniela Gmeinbauer".to_string(),
                "Olga Voglauer".to_string(),
                "Alma Zadić".to_string()
            ]
        );
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxv_nrsitz_12_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXV/NRSITZ/12")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Moser".to_string(),
                "Schenk".to_string(),
                "Heinzl".to_string(),
                "Wimmer".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxv_nrsitz_14_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXV/NRSITZ/14")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Heinzl".to_string(),
                "Stefan".to_string(),
                "Barbara Rosenkranz".to_string(),
                "Moser".to_string(),
                "Musiol".to_string(),
                "Schenk".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxv_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXV/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Bayr".to_string(),
                "Auer".to_string(),
                "Bösch".to_string(),
                "Kunasek".to_string(),
                "Schellenbacher".to_string(),
                "Schrangl".to_string(),
                "Vilimsky".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxiv_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXIV/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Faul".to_string(),
                "Praßl".to_string(),
                "Pilz".to_string(),
                "Lunacek".to_string(),
                "Glawischnig-Piesczek".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxiii_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXIII/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Hagenhofer".to_string(),
                "Kräuter".to_string(),
                "Schopf".to_string(),
                "Stauber".to_string(),
                "Großruck".to_string(),
                "Peter Haubner".to_string(),
                "Mandak".to_string(),
                "Öllinger".to_string(),
                "Fichtenbauer".to_string(),
                "Kickl".to_string(),
                "Kurzmann".to_string(),
                "Lutz Weinzinger".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxii_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXII/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Gabriela Moser".to_string(),
                "Kurt Grünewald".to_string(),
                "Klaus Hubert Auer".to_string(),
                "Manfred Lackner".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxi_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXI/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Fasslabend".to_string(),
                "Amon".to_string(),
                "Gerhard Bauer".to_string(),
                "Lunacek".to_string(),
                "Platter".to_string(),
                "Parnigoni".to_string(),
                "Prammer".to_string(),
                "Nürnberger".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xx_nrsitz_30_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XX/NRSITZ/30")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Brinek".to_string(),
                "Frieser".to_string(),
                "Gatterer".to_string(),
                "Gusenbauer".to_string(),
                "Kukacka".to_string(),
                "Elmecker".to_string(),
                "Meischberger".to_string(),
                "Hans Helmut Moser".to_string(),
                "Mock".to_string(),
                "Rauch-Kallat".to_string(),
            ]
        );
    }
    #[tokio::test]
    async fn test_get_plenar_gegenstand_xx_nrsitz_110_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XX/NRSITZ/110")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Aumayr".to_string(),
                "Schaffenrath".to_string(),
                "Mertel".to_string(),
                "Silhavy".to_string(),
                "Löschnak".to_string(),
                "Fuhrmann".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxii_nrsitz_110_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXII/NRSITZ/110")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Großruck".to_string(),
                "Wimmer".to_string(),
                "Öllinger".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxiv_nrsitz_110_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXIV/NRSITZ/110")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Keck".to_string(),
                "Schüssel".to_string(),
                "Jarmer".to_string(),
                "Bellen".to_string(), // 3 char words are removed: "Van der" Bellen
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvi_nrsitz_69_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVI/NRSITZ/69")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Barbara Krenn".to_string(),
                "Nico Marchetti".to_string(),
                "Josef Muchitsch".to_string(),
                "Dagmar Belakowitsch".to_string(),
                "Petra Steger".to_string(),
                "Beate Meinl-Reisinger".to_string(),
                "Alfred Noll".to_string(),
            ]
        );
    }
    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxviii_nrsitz_69_absences() {
        simple_logger::init_with_level(log::Level::Info).unwrap();
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/69")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec![
                "Manuel Litzke".to_string(),
                "Reinhold Maier".to_string(),
                "Volker Reifenberger".to_string(),
                "Christoph Steiner".to_string(),
                "Wolfgang Zanger".to_string(),
                "Gudrun Kugler".to_string(),
                "Carmen Jeitler-Cincelli".to_string(),
                "Christoph Stark".to_string(),
                "Petra Bayr".to_string(),
                "Elke Hanel-Torsch".to_string(),
                "Veit Valentin Dengler".to_string(),
                "Fiona Fiedler".to_string(),
            ]
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxviii_nrsitz_41_absences() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/41")
            .await
            .unwrap();

        assert_eq!(
            plenar.absences.clone().unwrap().absences,
            vec!["Elisabeth Heiß".to_string(), "Reinhold Binder".to_string(),]
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
            // "/dokument/XXVII/NRSITZ/211/A_-_12_03_00_00295752.html"
            "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/211/fnameorig_1598890.html#RU_295752"
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
            // "/dokument/XXVII/NRSITZ/211/A_-_15_24_25_00295995.html"
            "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/211/fnameorig_1598890.html#R_295995_4"
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
        return;
        unsafe {
            std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        }
        let mut scraper = AustriaScraper::default();

        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                let plenar = scraper.get_plenar(&path).await.unwrap();
                let mut extractor = TextExtractor::new();

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
        unsafe { std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false") };
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXII/NRSITZ/117")
            .await
            .unwrap();
        let mut extractor = TextExtractor::new();

        let mut lengths = vec![];
        let mut required_lengths = [
            14849, 12113, 9962, 12434, 9509, 11478, 10657, 9885, 9146, 9220, 6056, 5109,
            5084, // Row checked for exact speeches
            4511, 7507, 5851, 7427, 5631, 2145, 1786, 1593, 3360, 4653, 693, 3012, 3531,
            1238, // Row checked for exact speeches
            4336, 2845, 2945, 2413, 5636, 5753, 7728, 5483, 171, 5930, 4689, 2682, 2906, 2063,
            4767, 3727, 2943, 3815, 4253, 2113, 2153, 2614, 1779, 2367, 2469, 2183, 8286, 3212,
            6162, 1723, 2025, 3330, 4470, 1565, 1629, 1812, 2780, 1222, 2148, 1544, 2220, 2076,
            763, 2246, 3011, 2717, 1397, 2045, 1989, 2416, 1823, 1792, 2173, 2071, 1974, 1653,
            5448, 2087, 4080, 1604, 2214, 1940, 4432, 2082, 3246, 7223, 2374, 2468, 2442,
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
                    // panic!("{} required: {}", speech_text.len(), required_length)
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

        println!("lengths: {lengths:?}");
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_117() {
        unsafe { std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false") };
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

        let mut extractor = TextExtractor::new();

        let mut lengths = vec![];
        let mut required_lenghts = [
            10832, 5661, 3878, 8295, 11226, 9298, 8087, 3890, 4564, 4038, 4950, 4216, 3592, 2600,
            2739, 1902, 2150, 6368, 4873, 4830, 5002, 6103, 9346, 4242, 7348, 2641, 3348, 2743,
            2917, 3964, 440, 3532, 2464, 3945, 2896, 6627, 5222, 4424, 2266, 2860, 3043, 2953,
            4623, 3139, 4498, 2449, 1973, 1532, 5548, 2939, 2135, 1875, 4010, 3828, 18815, 5729,
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
        // let mut extractor = TextExtractor::new();

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
        let mut extractor = TextExtractor::new();

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
    async fn test_plenar_xxvii_189() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/189")
            .await
            .unwrap();
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxvii_243() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/243")
            .await
            .unwrap();
        println!("plenar: {plenar:?}");
    }

    #[tokio::test]
    async fn test_plenar_xxiv_55() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXIV/NRSITZ/55")
            .await
            .unwrap();

        let req_lengths = [
            10116, 1240, 5362, 4587, 5923, 3917, 5936, 5201, 3820, 4742, 5778, 6082, 4654, 5098,
            3544, 3359, 4444, 6437, 4856, 3947, 3528, 3012, 2730, 3648, 4967, 3886, 2770, 3469,
            3607, 3688, 1720, 2483, 3433, 2512, 8295, 3782, 3010, 4917, 4390, 4314, 2258, 2040,
            2495, 4400, 2988, 3387, 2095, 3035, 2355, 4046, 3016, 5154, 5052, 4445, 6002, 3444,
            3294, 5248, 4778, 612, 4517, 2069, 3846, 1838, 4630, 2430, 6422, 21490, 36132, 9803,
            7917, 7437, 6809, 7348, 10395, 1029, 8710, 5951, 5517, 7636, 5264, 6929, 6866, 4361,
            6025, 5479, 6707, 3872, 4411, 5194, 3364, 2646, 4269, 2873, 10117, 3752, 3416, 5095,
            4141, 4791, 3497, 5242, 5668, 9795, 3035, 2809, 2103, 3752, 783, 2620, 2822, 2610,
            2572, 1139, 3243, 2186, 3026, 2342, 5423, 6178, 5338, 2150, 1882, 2739, 2484, 3642,
            3760, 2227, 651, 1157, 1926, 3337, 3064, 2499, 1661, 1996, 3511, 3105,
        ];
        let mut text_extractor = TextExtractor::new();
        let mut lengths = vec![];
        let mut i = 0;
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
                lengths.push(speech_text.len());
                let required_length = req_lengths[i];
                i += 1;
                if (speech_text.len() as f32 / required_length as f32 - 1.).abs() >= 0.04 {
                    println!(
                        "{} -------------------------------------- {}{}",
                        &speech.speech.delegate_display_name,
                        AustriaScraper::BASE_URL,
                        speech.speech.spoken_text_html_urls[0].document_url
                    );
                    println!("speech_text: {speech_text}");
                    println!("--------------------------------------");
                    panic!("{} required: {}", speech_text.len(), required_length)
                }
                // println!("speech_text: {speech_text:?}");
            }
        }
        println!("lengths: {lengths:?}");
    }

    #[tokio::test]
    async fn test_speech_herbert_scheibner() {
        let speech = Speech {
            delegate_id: 1604,
            delegate_display_name: "Herbert Scheibner (B)".into(),
            opinion: Opinion::Against,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: Some(96),
                site_end: None,
                document_url: "https://www.parlament.gv.at/dokument/XXIV/NRSITZ/55/fnameorig_185240.html#RU_68072".into(),
            }],
        };

        let mut text_extractor = TextExtractor::new();

        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();
        assert_eq!(speech_text.len(), 3782);
    }

    #[tokio::test]
    async fn test_plenar_xxv_55() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXV/NRSITZ/55")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new();
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

    #[tokio::test]
    async fn test_plenar_xxviii_35() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/35")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new();
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

    #[tokio::test]
    async fn test_plenar_xxvi_43() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVI/NRSITZ/43")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new();
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                dbg!(speech);
                if let Some(speech_info) = speech.speech.spoken_text_html_urls.first() {
                    let speech_text = text_extractor
                        .speech_extraction_with_fit(
                            speech_info,
                            &speech.speech.delegate_display_name,
                        )
                        .await
                        .unwrap();
                    println!("speech_text: {speech_text:?}");
                } else {
                    println!("no speech info: {:?}", speech.speech);
                }
            }
        }
        // println!("plenar: {plenar:?}");
    }

    /*

    */

    #[tokio::test]
    async fn test_plenar_speech_diesner_martina() {
        // let mut extractor = TextExtractor::new();

        let mut text_extractor = TextExtractor::new();
        let speech = Speech {
            delegate_id: 16218,
            delegate_display_name: "Martina Diesner-Wais (V)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: None,
                site_end: None,
                document_url:
                    "https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/3/fnameorig_1688660.html#86"
                        .into(),
            }],
        };

        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap_or_default();
        let truth  ="Martina Diesner-Wais (ÖVP): Sehr geehrte Frau Präsidentin! Liebe Abgeordnete! Liebe Zuseher hier im Parlament und auch vor den Fernsehgeräten! Ich möchte gleich zu Beginn im Namen meiner Kollegin Bettina Zopf den Seniorenbund von Neukirchen begrüßen. – Willkommen im Parlament! (Beifall bei der ÖVP sowie bei Abgeordneten von FPÖ und SPÖ. – Präsident Rosenkranz übernimmt den Vorsitz.)
Meine Damen und Herren, wir befassen uns heute mit der Nachnominierung und der Wahl eines neuen Mitglieds der Volksanwaltschaft. Diese Nachnominierung ist notwendig geworden, weil Herr Dr. Rosenkranz, der bisher Volksanwalt war, in das Nationalratspräsidium gewechselt ist.
Unsere Volksanwaltschaft ist ein unabhängiges Kontrollorgan und hat eine tragende Rolle in der demokratischen Kultur in unserem Land. Die Volksanwaltschaft ist Ansprechpartner für Bürgerinnen und Bürger, die Missstände in der Verwaltung sehen und aufzeigen wollen, und sorgt dafür, dass die Verwaltung auch weiterhin fair, bürgernah und rechtskonform bleibt.
Die Volksanwaltschaft ist niederschwellig erreichbar. Sie können sie entweder an Sprechtagen telefonisch, über das Internet oder natürlich auch persönlich erreichen. Sie ist eine Institution, die dazu beiträgt, das Vertrauen der Bevölkerung in unsere staatlichen Einrichtungen zu stärken.
Die Volksanwaltschaft ist aber auch zum Schutz der Menschenrechte da und wird dahin gehend auch immer wieder tätig. Die Aufgabenbereiche, die Dr. Walter Rosenkranz in der Volksanwaltschaft innehatte, wird nun unsere neue Volksanwältin übernehmen, und dies sind wichtige Themenbereiche.
Sie umfassen Polizei, Fremden- und Asylrecht, Denkmalschutz, Land-, Forst- und Wasserwirtschaft, Natur- und Umweltschutz, Gewerbe und Betriebsanlagen, Kindergärten, Schulen, Universitäten, Verkehr, Gemeindeabgaben und auch Agrarangelegenheiten. Das ist eine große Bandbreite, und daher ist es eine anspruchsvolle und wirklich auch verantwortungsvolle Funktion.
Die Volksanwaltschaft ist ein Organ, der verlängerte Arm unseres Parlaments, und durch die Berichte der Volksanwaltschaft konnten wir hier auch schon viele Verbesserungen erzielen. Wir haben in Österreich einen verfassungsrechtlichen Bestellmodus, der besagt, dass die drei stärksten Parteien die Vorschläge für die Volksanwälte bringen können, und das gibt nun der FPÖ die Möglichkeit, Frau MMag. Elisabeth Schwetz vorzuschlagen.
Wir als ÖVP werden diesem Vorschlag zustimmen. Ich als Vorsitzende des Volksanwaltschaftsausschusses freue mich schon auf eine gute Zusammenarbeit, denn es ist wichtig, dass die Volksanwaltschaft und das Parlament gut zusammenarbeiten – im Sinne unserer Bürger und Bürgerinnen. (Beifall bei der ÖVP.)
14.59";

        assert!(compare_words(truth, &speech_text) > 0.997)
    }
    #[tokio::test]
    async fn test_plenar_speech_paul_stich() {
        // let mut extractor = TextExtractor::new();

        let mut text_extractor = TextExtractor::new();
        let speech = Speech {
            delegate_id: 30707,
            delegate_display_name: "Paul Stich (S)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: None,
                site_end: None,
                document_url: "https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/35/A_-_23_40_42_00818253.html#211".into(),
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
        let truth = "Abgeordneter Paul Stich (SPÖ): Vielen Dank, Herr Präsident! Sehr geehrte Frau Ministerin! Liebe Kolleginnen und Kollegen! Liebe Zuseherinnen und Zuseher, sofern Sie zu dieser späten Stunde noch auf sind! Die geopolitischen Konflikte auf der Welt spitzen sich zu, nicht zuletzt durch neu oder wieder aufs politische Parkett kommende scheinbare Kriegstreiber wie Donald Trump. Das zeigt sich in Debatten hier im Haus, das zeigt sich auch im Landesverteidigungsbericht, und das ergibt einerseits ganz viel an militärstrategischer Denkaufgabe und auf der anderen Seite aber auch vielfältige Aufgaben für das österreichische Bundesheer.

        Wenn wir über die geopolitischen Ereignisse sprechen, gilt es aus unserer Sicht vor allem eines herauszustreichen: dass es Krieg immer und überall mit allen Mitteln zu verhindern gilt. Es ist traurig, dass wir in einer Welt leben, in der diese Einschätzung nicht von allen und nicht überall geteilt wird, denn für manche scheint Krieg vielmehr wie eine Art Schachspiel zu sein. Die sitzen dann vor dem PC auf Twitter, meistens in ruhigen, recht gut geschützten Räumen, und diskutieren darüber, ob man nicht vielleicht doch irgendwo ein paar Leute opfern soll, quasi wie einen Bauern beim Schach irgendwo vorziehen sollte, um kurzfristig Menschen zu opfern, um langfristig daraus einen strategischen Vorteil zu ziehen. Ich glaube, gerade in diesem Zusammenhang ist es wichtig, da auch einen sehr bewussten Kontrastpunkt zu setzen, denn Krieg bringt Leid, Tod und Zerstörung, bringt Wunden in Familien, die über Generationen da sind, und wer beginnt, tote Zivilist:innen aufzurechnen, um daraus Legitimation für weitere Angriffe oder Tote zu schaffen, der wird menschlich am Ende des Tages immer verlieren. (Beifall bei der SPÖ.)

        Denn: Wie fühlt sich Krieg für diejenigen an, die ihn hautnah erleben? – Die meisten von uns, und zum Glück auch ich, können diese Frage nicht persönlich beantworten. Deshalb will ich mich der Worte der persönlichen Erinnerung von Almasa Salihović bedienen, einer Überlebenden des Massakers von Srebrenica und Mitarbeiterin im Srebrenica Memorial Center, die sie gestern im Rahmen einer sehr würdigen Gedenkveranstaltung hier im Parlament hat ausführen können. Sie sagt ganz konkret über die Schrecken des Krieges – ich zitiere –: Meine Mutter, meine Schwester und ich mussten mit ansehen, wie Männer und Jungen von den Soldaten getrennt wurden. Ich erinnere mich an das Schreien, an das Flehen, an die Hilflosigkeit. Mein Bruder Abdullah wurde nie wieder gesehen. Er war erst 18 Jahre alt. Er hatte versucht, mit meiner Schwester die Basis zu verlassen, wurde aber festgenommen. Aus ihrem Bericht weiß ich, sie sahen sich noch einmal an, bevor er abgeführt wurde. Sie wusste es, er wusste es, und nun weiß auch ich es: Das war ihr letzter Abschied. – Zitatende.

        Diese durchwegs berührenden Worte sind für uns aus meiner Sicht Handlungsauftrag auf zwei Ebenen: zum einen, weil das österreichische Bundesheer – und hier schließt sich zu einem gewissen Grad der Kreis – seit Jahrzehnten ja auch Friedenstruppen in Bosnien stellt, um den Frieden von 1995 abzusichern, was ein sehr gutes Beispiel dafür ist, wie man ganz besonders auch als neutraler Staat einen wichtigen Beitrag in der internationalen Gemeinschaft leistet, ganz besonders in einer Situation, in der sich auch in Bosnien innenpolitisch die Lage durchwegs zuspitzt; und zum anderen, weil es natürlich auch ein politischer Auftrag ist, der untrennbar mit der militärischen Debatte, die wir hier führen, verbunden ist, nämlich dass wir sehen, dass die Tendenz immer mehr wieder dahin geht, dass Krieg und außenpolitisch-militärische Schläge zu einem tolerierten Mittel der globalen Außenpolitik werden. Diese Entwicklung ist brandgefährlich, und wir werden im Rahmen dieser Plenartage auch noch über die Kandidatur Österreichs für den UN-Sicherheitsrat sprechen. Lassen Sie mich nur so viel vorgreifen: Ich halte das für einen guten, richtigen und wichtigen Schritt, damit die Stimmen der Deeskalation und der friedlichen Koexistenz ganz besonders im Sinne der österreichischen Neutralitätspolitik auch international wieder gestärkt werden (Beifall bei der SPÖ), damit Soldaten dieser Welt – egal ob im österreichischen Bundesheer oder in allen anderen Heeren – die Schlachtfelder weiterhin nur aus den Geschichtsbüchern kennen. – Vielen Dank. (Beifall bei der SPÖ und bei Abgeordneten der ÖVP.)";

        dbg!(compare_words(truth, &speech_text));
        assert!(compare_words(truth, &speech_text) > 0.997)
    }
    // println!("plenar: {plenar:?}");

    #[tokio::test]
    async fn test_plenar_xxviii_3() {
        // let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/3")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new();
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
    async fn test_plenar_speech_dieter_brosz_nrsitz_117() {
        let speech = Speech {
            delegate_id: 8239,
            delegate_display_name: "Dieter Brosz, Msc (G)".into(),
            opinion: Opinion::Against,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: Some(15),
                site_end: Some(19),
                document_url:
                    "https://www.parlament.gv.at/dokument/XXII/NRSITZ/117/SEITE_0015.html".into(),
            }],
        };

        let mut text_extractor = TextExtractor::new();
        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();
        let truth = "Dieter Brosz, Kolleginnen und
        Kollegen betreffend Bericht über die Umsetzungskosten der Empfehlungen der
        Zukunftskommission (1046 d.B.)
        3. Punkt
        Bericht des Unterrichtsausschusses über
        den Antrag 630/A (E) der Abgeordneten Dieter Brosz, Kolleginnen und
        Kollegen betreffend Rechtsanspruch auf Nachmittagsbetreuung (1047 d.B.)
        4. Punkt
        Bericht des Unterrichtsausschusses über
        den Antrag 589/A (E) der Abgeordneten Dieter Brosz, Kolleginnen und
        Kollegen betreffend Kassasturz im Schulbereich (1049 d.B.)

        Präsident Dr.
        Andreas Khol: Wir gelangen nun zu den Punkten 1 bis 4 der Tagesordnung,
        worüber die Debatte unter einem durchgeführt wird.
        Auf eine mündliche Berichterstattung wurde
        verzichtet.
        Wir gehen in die Debatte ein.
        Zu Wort gemeldet hat sich Herr Abgeordneter
        Brosz. Seine Redezeit beträgt 12 Minuten. – Herr Kollege, Sie sind
        am Wort. – Kurz vor Ende der Redezeit gebe ich ein Glockenzeichen, dann
        wissen Sie: Schlusssatz!

        9.19.10
        Abgeordneter
        Dieter Brosz (Grüne): Herr Präsident!
        Frau Bundesministerin! Wir diskutieren heute das so genannte
        Schulpaket 1, jene Maßnahmen, die Sie als Bundesministerin als Antwort
        vor allem auch auf die Ergebnisse der PISA-Studie setzen.
        Wenn man sich die PISA-Studie nochmals in
        Erinnerung ruft, dann weiß man, da gab es einige ganz zentrale Aussagen, die
        auf das österreichische Bildungssystem kein besonders gutes Licht geworfen
        haben. Unter anderem haben 20 bis 25 Prozent der österreichischen
        SchülerInnen – 15- bis 16-Jährige – massive Defizite. Sie haben massive
        Defizite im Sprachbereich, im mathematischen Bereich. Das ist also eine große
        Zahl von Schülerinnen und Schülern, die in
        ihrer weiteren Berufskarriere de facto extrem große Schwierigkeiten haben wird.
        (Unruhe im Saal.)
        Ich muss feststellen, dass für diese
        SchülerInnen Ihr Schulpaket keine einzige Maßnahme vorsieht.

        Präsident Dr.
        Andreas Khol: Meine Damen und Herren! Ich würde bitten, den Geräuschpegel zu
        senken und die Gespräche einzustellen! Man hört am Präsidium vor lauter
        Gemurmel den Redner nicht.
        Bitte, Herr Abgeordneter Brosz.

        Abgeordneter Dieter
        Brosz (fortsetzend): Der zweite zentrale Punkt: Kaum ein Land hat einen so großen
        sozialen Hintergrund im Schulbereich wie Österreich. Die österreichischen
        SchülerInnen sind extrem davon abhängig, wie die Situation in ihrem Elternhaus
        ist, was den Bereich Bildung und Einkommen betrifft. Wenn österreichische SchülerInnen
        Eltern haben, die über keine höhere Bildung verfügen, dann ist es nach wie vor
        so, dass ihre Chancen sehr beschränkt sind. Ich finde, dass das für ein so
        reiches Land wie Österreich ein Armutszeugnis ist. (Beifall bei den Grünen
        und bei Abgeordneten der SPÖ.)
        Der dritte große Punkt: Wir haben kein
        ausgebautes Fördersystem in Österreich. Private Nachhilfe ist eine der
        Wachstumsbranchen Österreichs. Es gibt kaum ein Gebiet, das so bunt ist. –
        Wirtschaftlich ja durchaus erfreulich. Die Frage ist nur: Wer zahlt es?, und:
        Kann ein Land damit zufrieden sein, dass es private Förderung, private
        Nachhilfe anstatt von schulischer Förderung gibt? – Ich glaube, Österreich
        hat sich ein besseres Schulsystem verdient. (Beifall bei den Grünen.)
        Der letzte Punkt, den ich heute besonders
        ansprechen will: Wir wissen, dass die AkademikerInnenrate in Österreich weit
        niedriger ist als jene in vielen anderen europäischen Ländern. Wir wissen,
        dass die Übertrittsraten von den Schulen an die Universitäten im
        internationalen Vergleich zurückhängen.
        Ich sage Ihnen nur ein Beispiel, weil wir
        heute über Zugangsbeschränkungen diskutieren. Sie wissen so gut wie ich, dass
        Finnland, das PISA-Siegerland, ein politisches Ziel definiert hat. Dieses
        politische Ziel lautet: 80 Prozent der MaturantInnen sollen einen
        Studienplatz in Finnland bekommen.
        Wir haben in Österreich Raten von
        30 Prozent und diskutieren jetzt, ob wir vielleicht in Richtung
        50 Prozent kommen können. Sie sehen, dass da ein massiver Nachholbedarf
        gegeben ist. Angesichts dieser Tatsache halte ich es wirklich für unzulänglich,
        über Zugangsbeschränkungen zu diskutieren, ohne die Zahl der Studienplätze
        auszuweiten. Diese Zugangsbeschränkungen werden dazu führen, dass es für weniger
        österreichische Studierende Plätze an den Universitäten geben wird, und das
        ist schlicht und einfach die falsche Antwort auf ein Problem, das wir seit
        Jahren kennen. (Beifall bei den Grünen und bei Abgeordneten der SPÖ.)
        Nachdem ich jetzt über das gesprochen habe,
        was eigentlich in diesem Schulpaket drinnen sein sollte, aber nicht drinnen
        ist, worüber wir heute auch nicht abstimmen können, weil wir nur über das
        abstimmen können, was uns vorgelegt wird, komme ich jetzt zum Kern der Materie.
        Es handelt sich um eine ganze Reihe von Veränderungen, die im Wesentlichen
        formale Anpassungen sind, wie zum Beispiel die von Ihnen heiß ersehnte
        Umbenennung des Unterrichtsgegenstandes „Leibesübungen“ in „Bewegung und
        Sport“. Wir können alle froh darüber sein, das ist eine „große Entwicklung“ im
        Schulsystem. Es werden die Sonderschulen in Förderschulen umbenannt – auch
        das ist offenbar ein „großer Wurf“.

        Es gibt zwei Dinge, die es verdienen, näher
        angeschaut zu werden. Das ist zum einen die Fünf-Tage-Woche in den
        Pflichtschulen. Auch da muss man sagen, über 90 Prozent der
        Pflichtschulen waren bereits fünftägig geführt; bei den restlichen wird das
        jetzt der Fall sein. Ich glaube, daran sieht man, dass die Qualität des
        Schulsystems dadurch nicht drastisch steigen wird. Aber der zentrale Punkt, der
        drinnen ist, ist die Nachmittagsbetreuung. Ich sage es gleich vorweg: Die
        Grünen werden diesem Gesetzesantrag deshalb nicht zustimmen, weil die
        Nachmittagsbetreuung völlig unzulänglich geregelt ist. (Beifall bei den
        Grünen. – Widerspruch bei der ÖVP.)
        Wenn Sie von der ÖVP hier zwischenrufen,
        eine ganz simple Frage: Was hindert Sie daran, in den österreichischen Gesetzen
        einen Rechtsanspruch auf Nachmittagsbetreuung sicherzustellen? Wir hätten über
        jede Form diskutiert, die das sicherstellt. Wir haben nicht gefordert, dass an
        jeder einzelnen Schule Nachmittagsbetreuung sein muss. Wir haben auch nicht
        gefordert, dass es nur ganztägig geführte Schulen sein müssen, wo jeder Schüler
        oder jede Schülerin den ganzen Tag in der Schule sein muss. All das hätten wir
        diskutieren können.
        Das Einzige, auf das wir bestanden haben
        und wo Sie gesagt haben, das geht nicht, ist ein Rechtsanspruch. Jetzt machen
        Sie folgende Regelung: dass ab 15 SchülerInnen, die angemeldet sind –
        übrigens in diesem Fall an einer Schule –, Nachmittagsbetreuung angeboten
        werden muss. Dann gibt es noch eine sehr große Hürde, wenn man wirklich
        Ganztagsschulen führen will, wo nämlich verschränkter Unterricht mit besseren
        Erholungspausen, mit der Möglichkeit von Wechsel zwischen Unterricht und Fördermaßnahmen
        gegeben ist: Da müssen dann zwei Drittel der Eltern am Schulstandort zustimmen.
        Zwei Drittel der Eltern müssen zustimmen, dass eine Ganztagsschule geführt
        werden kann! Das ist eine Hürde, die Sie an kaum einer österreichischen Schule
        überwinden können. Ein Rechtsanspruch darauf besteht in keiner Form.
        Unser Vorschlag wäre gewesen: Jeder
        Elternteil, jede SchülerIn soll das Recht haben, zu wählen, ob sie eine
        herkömmliche Schule besucht, ob sie eine Schule mit Nachmittagsbetreuung, also
        Unterricht am Vormittag und Nachmittagsbetreuung danach, wünscht oder ob es
        eine wirkliche Ganztagsschule sein soll – und das in einer angemessenen
        Entfernung zum Wohnort. Dieses Recht verwehren Sie den österreichischen
        Eltern, den SchülerInnen nach wie vor, und daher ist dieses Gesetz völlig unzulänglich.
        (Beifall bei den Grünen.)
        Der zweite wichtige Punkt:
        15 SchülerInnen Minimum, damit die Gemeinden zusätzliches Geld bekommen.
        Natürlich kann jede Gemeinde eine Schule eröffnen, auch Nachmittagsbetreuung
        anbieten, wenn sie es selbst zahlt. Wenn die Gemeinde selbst zahlt, können Sie
        für zwei SchülerInnen auch eine Nachmittagsbetreuung machen. Ich würde Sie
        ersuchen, dass Sie uns dann die Anzahl jener österreichischen Gemeinden nennen,
        die sagen, wir haben zwar keine 15, wir haben viel weniger, dort machen wir es
        trotzdem. Die wird es nicht wirklich geben. Aber wen benachteiligt denn diese
        Regelung? – Die Regionen, für die Sie sich scheinbar immer mit großem
        Enthusiasmus einsetzen, nämlich die ländlichen Regionen. In den Städten wird es
        ein relativ geringes Problem sein – da kann man ja auch die Schulen
        auswählen –, Schulen zu finden, wo es Nachmittagsbetreuung gibt.
        Wenn in ländlichen Regionen keine
        15 Schüler angemeldet sind, gerade diese Gemeinden aber mittlerweile
        große finanzielle Probleme bekommen haben, dann wird es dort das Angebot
        schlicht und einfach nicht geben. Sie von der ÖVP brauchen uns nicht mehr damit
        zu kommen, dass Sie sich für die ländlichen Regionen einsetzen. (Beifall bei
        den Grünen.) Das ist einfach der nächste Schritt der Benachteiligung und
        der Ausdünnung genau dieses Raums, das ist für uns auch nicht nachvollziehbar.

        Die Sicherheit: Die Grünen arbeiten im
        Übrigen an Modellen, wonach auch Karenz von Vätern viel stärker in Anspruch
        genommen werden soll; das ist in Österreich de facto kaum der Fall. Wir sind,
        glaube ich, im Moment bei 2 Prozent Väterkarenz. Wir wollen, dass es da
        klare partnerschaftliche Modelle gibt, aber zurzeit ist es schlicht und einfach
        so, dass der Großteil der Kinderbetreuung nach wie vor bei den Frauen bleibt
        und von den Frauen gewährleistet werden muss. Wenn Sie sagen, ab 15 wird
        finanziert, dann frage ich Sie, wie Sie gewährleisten wollen, dass von einem
        Jahr aufs andere nicht die Situation eintritt, dass eine bestehende
        Nachmittagsbetreuung nicht mehr da ist. Sie müssen sich vorstellen, die Eltern
        entscheiden sich in ihrem beruflichen Umfeld: Okay, ich gebe mein Kind in diese
        Schule, und mein Kind hat dort eine Nachmittagsbetreuung. – Wenn im Jahr
        darauf keine 15 Eltern mehr da sind, die ihre Kinder anmelden, dann können
        sie ein Pech haben, dann gibt es keinen Rechtsanspruch mehr und dann gibt es
        auch keine Nachmittagsbetreuung mehr.
        Ich würde Ihnen einmal selbst das Vergnügen
        wünschen, dass Sie von Jahr zu Jahr nicht wissen, wie Sie Ihre berufliche
        Tätigkeit planen können, und dass Sie von Jahr zu Jahr nicht wissen, ob für
        Ihre Kinder ein Betreuungsplatz da ist. Dann versuchen Sie einmal, Beruf und
        Familie zu vereinbaren. Viel Glück, Frau Bundesministerin! (Beifall bei den
        Grünen und bei Abgeordneten der SPÖ.)
        Sie erzählen uns ja immer, wie furchtbar
        Deutschland ist – offenbar ein Land, über das man grundsätzlich nur
        politisch negativ reden kann. Deutschland hat die Ergebnisse der ersten
        PISA-Studie verarbeitet. – Ich habe im Übrigen jetzt in Deutschland Folgendes
        gelesen. Da ist gestanden: Die schlechten Ergebnisse der PISA-Studie: In Österreich
        hat man gesehen: „Weltklasse!“, bevor es schlechtere Ergebnisse gegeben hat als
        in Deutschland. – Deutschland hat gesagt: Was müssen wir machen? –
        Ein Schwerpunkt der deutschen Bundesregierung war der Ausbau der
        Ganztagsschulen. Die deutsche Bundesregierung hat für die Jahre 2003 bis
        2007 ein 4 Milliarden €-Paket für den Ausbau von ganztägigen
        Schulformen geschnürt. Jetzt kann man sagen, Deutschland ist etwa zehnmal so
        groß wie Österreich, also reduzieren wir es um den Faktor 10. Dann kämen
        wir auf 400 Millionen € für Österreich innerhalb von fünf Jahren,
        wenn man den deutschen Standard nimmt.
        Wissen Sie, was Sie budgetiert
        haben? – 8 Millionen € pro Jahr! Das ist ein Zehntel dessen, was
        Deutschland in den Jahren 2003 bis 2007 finanziert hat und finanzieren
        wird. Ich sage Ihnen noch etwas: In Deutschland ist das ausschließlich, und
        zwar zusätzlich, für die Adaption der Schulen zur Verfügung gestellt worden.
        Dafür gibt es im Bundesbudget überhaupt kein Geld, weil Sie sagen: Erstens sind
        österreichische Schulen so ausgestattet, dass man Kinder guten Gewissens dort
        den ganzen Tag reingeben kann! – Schulen, die für Nachmittagsbetreuung
        errichtet wurden, wo es weder eine Küche noch eine Essensmöglichkeit gibt?! Die
        Kinder können dann in ihren Bänken wahrscheinlich das Mittagessen von
        McDonald’s einführen und dort speisen. – Das ist nicht die Form von
        Nachmittagsbetreuung und von Ganztagsschulen, wie wir sie uns vorstellen! (Beifall
        bei den Grünen. – Ruf bei der ÖVP: Die arbeitslosen Lehrer in
        Deutschland!)

        Präsident Dr.
        Andreas Khol: Herr Abgeordneter, wollen Sie nicht zwei Abänderungsanträge
        einbringen?

        Abgeordneter Dieter
        Brosz (fortsetzend): Die bringen wir nachher ein.
        Letzter
        Punkt, der mir in diesem Zusammenhang sehr wichtig ist: die soziale Gerechtigkeit
        bei der Nachmittagsbetreuung. Soziale Staffelung, sagen Sie, gibt es in Österreich,
        in den Bundesschulen. – Ja, sie gibt es. Und für alle, die jetzt glauben,
        damit haben jene, die wirklich wenig Geld verdienen, einen Anspruch darauf,
        dass sie auch
        kostenlose
        Nachmittagsbetreuung bekommen: Das Familieneinkommen pro Jahr beträgt
        10 200 €; darunter braucht man nichts zu zahlen.
        Wenn eine Familie pro Jahr mehr als
        10 200 € verdient – Familieneinkommen, zwei Verdienende –,
        wird gezahlt. Und wissen Sie, wo die soziale Grenze endet? – Bei
        17 000 € Familieneinkommen pro Jahr – mit Kindern! (Abg. Parnigoni:
        Das ist ja unglaublich!) Ab diesem Betrag gibt es keinen Euro mehr für
        eine soziale Staffelung bei der Kinderbetreuung.
        Wissen Sie, was 17 000 € bei Familien
        mit einem Kind oder zwei Kindern sind? – Das kann ich Ihnen sagen:
        Armutsgefährdung. Schlicht und einfach Armutsgefährdung. (Abg. Öllinger:
        Das ist nicht Armutsgefährdung, das ist komplette Armut!) Menschen, die zu
        zweit plus mindestens ein Kind nicht mehr als 17 000 € im Jahr haben,
        sind an der Grenze der Armut.
        Sie aber sagen: Wir haben ein faires
        System. Jeder, der kein Geld hat, wird bei uns gefördert. – Das ist in
        Österreich schlicht und einfach nicht der Fall! (Beifall bei den Grünen und
        der SPÖ. – Abg. Parnigoni: Die Frau Minister hat 17 000 im
        Monat!)
        Eine abschließende Bemerkung zur Integration.
        Wir haben jetzt zu wenig Plätze in der Nachmittagsbetreuung. Das führt dazu,
        dass jene Eltern, die die Möglichkeit haben, ihre Kinder zu Hause zu betreuen,
        die Ersten sind, die keine Nachmittagsbetreuungsplätze bekommen. Wir wissen,
        dass die Berufstätigkeit bei Migrantinnen geringer ist als bei
        Österreicherinnen. (Präsident Dr. Khol gibt das Glockenzeichzen.) Das
        führt dazu, dass Kinder von Migrantinnen die Ersten sind, die keine
        Nachmittagsbetreuungsplätze bekommen – trotz der notwendigen sprachlichen
        Förderung!
        Frau Bundesministerin, dieses Paket zur
        Nachmittagsbetreuung verdient unsere Zustimmung nicht! (Beifall bei den
        Grünen.)";

        assert!(compare_words(truth, &speech_text) > 0.97);
        // println!("speech_text: {speech_text}");
    }

    #[tokio::test]
    async fn test_elisabeth_gehrer_speech() {
        let speech = Speech {
            delegate_id: 3131,
            delegate_display_name: "BM Elisabeth Gehrer (V)".into(),
            opinion: Opinion::Other("Regierungsbank".into()),
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: Some(29),
                site_end: Some(31),
                document_url:
                    "https://www.parlament.gv.at/dokument/XXII/NRSITZ/117/SEITE_0029.html".into(),
            }],
        };

        let mut text_extractor = TextExtractor::new();
        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();

        let truth = "Elisabeth Gehrer: Herr Präsident! Hohes Haus! Ich freue mich, dass wir heute einen weiteren Schritt in der Steigerung der Schulqualität machen können – einen weiteren Schritt nach vielen, die wir schon gesetzt haben! (Beifall bei der ÖVP und den Freiheitlichen.)
        Wer mit offenen Augen durch unsere Schullandschaft geht, der sieht, was in den letz­ten Jahren geleistet wurde (Ruf bei der SPÖ: Ja, leider!), was an neuen Schulen ge­baut wurde. Ich danke herzlich allen Schulerhaltern, allen Bürgermeistern und Bürger­meisterinnen, allen in den Landesregierungen Verantwortlichen. Unsere Schulbauten können sich sehen lassen! (Beifall bei der ÖVP und den Freiheitlichen.)
        Ich freue mich auch, dass wir im Bundesschulbereich in Zusammenarbeit mit der BIG und mit anderen Körperschaften die besten Gebäude mit bester Qualität errichten kön­nen, die sogar Architekturpreise gewinnen! Das ist eine schöne Leistung für unsere Architekten, das ist eine schöne Leistung für unsere Schullandschaft.

        Meine Damen und Herren! Die Schulqualität hängt aber in besonderem Maße von den qualifizierten Lehrern und Lehrerinnen ab, und ich betone es: Wir haben in Österreich 116 000 qualifizierte Lehrer und Lehrerinnen, denen ich für ihre Leistungen im abge­laufenen Schuljahr sehr, sehr herzlich danken möchte! (Beifall bei der ÖVP und den Freiheitlichen.)

        Heute ist auch in den westlichen Bundesländern der letzte Schultag. 750 000 Schüler und Schülerinnen treten ihre wohlverdienten Ferien an. Insgesamt haben jetzt 1,2 Mil­lionen Schüler und Schülerinnen Ferien. Ich wünsche ihnen eine gute Erholung (Abg. Scheibner: Den Lehrern oder den Schülern?) und freue mich, dass es heuer weniger Repetenten gibt als im letzten Jahr. – Ich wünsche auch den Schülern eine gute Erholung. (Beifall bei der ÖVP und bei Abgeordneten der Freiheitlichen.)

        Wer mit denselben offenen Augen, mit denen er die Schulgebäude anschaut, auch die Qualität unseres Schulwesens anschaut, der sieht, was in den letzten Jahren – in vie­len Fällen gemeinsam, und das ist gut so! – an Veränderungen bewirkt wurde. (Abg. Dr. Jarolim: Das hat man gesehen, leider Gottes! Dramatisch hat man das gesehen! – Rufe bei der ÖVP: Geh bitte!) Wir haben die Autonomie ausgebaut; die Schulen sind selbständiger. Wir haben neue Lehrpläne geschaffen, wir haben die Projektarbeit, die modernen Unterrichtsformen vorangetrieben. Wir haben die Auslandsaufenthalte im Rahmen der Mobilitätsprogramme in Europa enorm gesteigert. Wir haben die Schulen mit Computern ausgestattet (Abg. Mag. Gaßner: Welche Schule?): Österreich liegt in Bezug auf die Computerausstattung an der Spitze. Wir haben eine neue Qualitätskultur mit Feedbackbögen und dem Projekt QIS eingeführt.

        Was mich besonders freut: Der Trend zur höheren Bildung hält an! Wir haben 40 425 Schüler und Schülerinnen mehr in weiterführenden Schulen, und wir werden daher auch mehr Studierende an den Universitäten haben. (Abg. Brosz: Aber nicht mehr Plätze!) Das ist ein schöner Bildungserfolg! (Beifall bei der ÖVP und den Freiheit­lichen.)

        Meine Damen und Herren! Wir haben gemeinsam das Frühwarnsystem vorverlegt – und wir haben die Repetentenzahlen gesenkt: In den AHS sind sie um 2 Prozent ge­sunken, in den berufsbildenden Schulen um 3,8 Prozent. Das freut mich! Es bedeutet weniger Misserfolgserlebnisse für manche Schüler und Schülerinnen.

        Wir haben die Lesefähigkeit der SchülerInnen in den Mittelpunkt gestellt: Es wurden im Monat Mai bei 189 261 Schülern und Schülerinnen Lese-Screenings durchgeführt. Etwa 40 Prozent lesen sehr gut, etwa 40 Prozent lesen gut, und 15 Prozent werden im nächsten Schuljahr jene Förderung kriegen, die sie brauchen, damit ihre Lesekompe­tenz verbessert wird.

        Wer aber meint, dass die Schulqualität nur mit gesetzlichen Veränderungen bewirkt werden kann, der ist auf einem Auge blind! (Abg. Öllinger: Besser auf einem Auge blind als auf zwei!) Sie wissen, dass in vielen Bereichen viele Projekte, viele Maßnah­men durchgeführt werden, die nicht gesetzlich verankert, aber unglaublich wichtig sind. Ich erwähne etwa die Einführung von Standards, Standards, die bereits erprobt wer­den; ich erwähne die Leadership Academy, ich erwähne jene Diskussion, die im Rahmen der von mir übrigens bereits im Jahr 2003 eingesetzten Zukunftskommission eine starke Bewegung erzeugt hat. Ich erwähne die Förderstunden, die neu angeboten werden: 36 Stunden in jeder Volksschulklasse, 72 Stunden in jeder Hauptschulklasse, 72 Stunden pro Jahr in jeder Unterstufe des Gymnasiums.

        Heute nun beschließen wir einige wichtige gesetzliche Weiterentwicklungen. Es ist ein Schulpaket, das für die Schulen und die SchülerInnen wichtige Weiterentwicklungen bringt. Meine Vorredner haben bereits die gesamte Maßnahmenpalette erwähnt, ich möchte deshalb auf einige mir besonders wichtige Fragen eingehen.

        Es wurde in breitem Maße die so genannte Tagesbetreuung diskutiert. – Meine Damen und Herren, ich verstehe schon, dass manche bei den Grünen oder in der Opposition nicht gerne hören, dass ihre verpflichtende Tagesschule, die Ganztagsschule, von den Österreichern und Österreicherinnen flächendeckend halt einfach nicht gewollt wird.

        Wir haben das sehr genau abgefragt: 77 Prozent der Befragten sind für eine freiwillige Tagesbetreuung, die auf die Bedürfnisse der berufstätigen Eltern abstellt. (Abg. Öllin­ger: Das kommt so auf die Frage an, das wissen Sie genau!) 62 Prozent sind gegen eine verpflichtende flächendeckende Ganztagsschule, mit der wir allen Kindern verord­nen, dass sie von 8 Uhr in der Früh bis 17 Uhr am Nachmittag in der Schule sein müs­sen. (Abg. Öllinger: Jetzt sind Sie wieder bei Ihren alten Programmen! – Ruf: Das ist die „Zwangstagsschule“!) Ich sage Ihnen: Die Tagesbetreuung wird von denen, die es brauchen, sehr gut angenommen!

        Ich fordere diejenigen, die davon reden, wir bräuchten lauter neue Gebäude, wir bräuchten lauter Umbauten, wir bräuchten lauter neue Küchen, wir bräuchten lauter Speisesäle, wir würden den ländlichen Raum aushöhlen, es werde im ländlichen Raum nicht möglich sein (Rufe bei den Grünen: Ja, ganz genau!), auf: Meine Damen und Herren! Bitte, gehen Sie in den ländlichen Raum und schauen Sie sich das doch an! (Abg. Sburny: Wir wohnen dort! Wir wohnen dort!) Sie reden ja weit weg von jeder Realität! (Beifall bei der ÖVP und bei Abgeordneten der Freiheitlichen. – Abg. Sburny: Ich weiß ja nicht, wo Sie wohnen ...! – Abg. Brosz: Der Minoritenplatz ist nicht länd­licher Raum!)

        Wissen Sie, als Bundesministerin kenne ich nicht nur meinen Wohnort Bregenz (Zwi­schenrufe bei den Grünen und der SPÖ), sondern ich kenne viele Schulen von innen, weil ich dorthin gehe, weil ich in die Bundesländer gehe, weil ich mit den Direktoren und Direktorinnen rede und weil ich mit den Eltern rede!

        Ich werde Ihnen ein Beispiel bringen (Abg. Sburny: Ja! – Abg. Öllinger: Bitte!): Ich war vor 14 Tagen in einer kleinen Gemeinde in Kärnten, in Gmünd, diese bietet die verschränkte Betreuung an. Wissen Sie, wie es die machen? – Die Kinder gehen zu Mittag in ein nahe gelegenes Gasthaus! Das wollen die Eltern so, und die Schüler und Schülerinnen bekommen dort ein gutes Essen zu einem äußerst günstigen Preis. Der Schulerhalter und die Schule haben das mit diesem Gasthaus vereinbart. – Sie wollen
        natürlich, dass man eine Riesenküche, dass man riesige Speisesäle baut. (Beifall bei der ÖVP und bei Abgeordneten der Freiheitlichen.)
        Ich bitte wirklich darum, bei all diesen Fragen ein bisschen Hausverstand walten zu lassen! (Beifall bei der ÖVP und den Freiheitlichen. – Staatssekretär Mag. Schweitzer: Haben sie ja nicht! Sehr gute Rede! – Abg. Sburny: Stellen Sie jetzt zu jeder Schule ein Gasthaus?)
        Und, meine Damen und Herren, ich weiß auch genau (Abg. Dr. Jarolim: Unseriöse Rede!), wie es in kleinen Gemeinden gehandhabt wird, wenn nur ein oder zwei Kinder eine Betreuung brauchen. (Abg. Öllinger: Ist das jetzt Nachmittagsbetreuung beim Wirten?) Die Gemeinde bemüht sich, sucht eine Tagesmutter und schaut, dass die Kinder in einer Familie mitbetreut werden. Was haben Sie eigentlich gegen rationale, vernünftige Lösungen, die von den Verantwortlichen vor Ort getroffen werden? (Abg. Sburny: Weil sie nicht funktionieren! Sie funktionieren nicht!) Ich bedanke mich bei allen, die das machen! (Beifall bei der ÖVP und bei Abgeordneten der Freiheitlichen.)
        Meine Damen und Herren! Ich möchte noch festhalten, dass wir nicht, wie da erwähnt wurde, 8 Millionen € im Jahr ausgeben. Wir werden, wenn wir 65 000 Betreuungsplätze haben, 62 Millionen € (Zwischenruf des Abg. Brosz) – 52 Millionen € ausgeben (Abg. Mandak: Naja, 10 Millionen mehr oder weniger!), und zwar zur Unterstützung der Ge­meinden bei der Betreuung der Kinder, zur Lernförderung der Kinder – und nicht für überflüssige Zubauten. (Beifall und Bravorufe bei der ÖVP und bei Abgeordneten der Freiheitlichen.)
        Meine Damen und Herren! Gute Schule besteht aus dem täglichen Bemühen, noch besser zu werden. Es genügt nicht, an einem Rad zu drehen, es genügt nicht, einen Hebel umzulegen, wir müssen viele Fortschritte machen! Wir haben bereits viele Fort­schritte gemacht, und wir machen nun einen großen Schritt mit dem heutigen Schul­paket.
        Ich danke denen, die zustimmen. Wir werden in Zukunft unsere Schulen in positiver Weise weiterentwickeln! (Lebhafter Beifall bei der ÖVP und den Freiheitlichen.)
        ";
        dbg!(compare_words(truth, &speech_text));
        assert!(compare_words(truth, &speech_text) > 0.96);
    }

    #[tokio::test]
    async fn test_speech_fritz_neugebauer() {
        let speech = Speech {
            delegate_id: 3604,
            delegate_display_name: "Fritz Neugebauer (V)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: Some(46),
                site_end: Some(48),
                document_url:
                    "https://www.parlament.gv.at/dokument/XXII/NRSITZ/117/SEITE_0046.html".into(),
            }],
        };

        let mut text_extractor = TextExtractor::new();
        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();

        let truth = "Abgeordneter Fritz Neugebauer (ÖVP): Frau Präsidentin! Frau Bundesministerin! Herr Staatssekretär! Liebe Kolleginnen und Kollegen! Liebe Frau Kollegin Sburny, ich

        lade Sie herzlich ein, die Sinnhaftigkeit einer Diskussion als Grundlage eines demokra­tischen Diskussionsprozesses nicht in Frage zu stellen!

        Sie müssen nicht mit jeder Äußerung des Herrn Staatssekretärs einverstanden sein, aber es macht immer Sinn, seine Argumente auf den Tisch zu legen. (Beifall bei der ÖVP und den Freiheitlichen. – Abg. Sburny: Ich habe es eh gemacht!)

        Liebe Kolleginnen und Kollegen! Dass Herr Kollege Broukal auf Grund seiner früheren Profession sehr sprachgewandt – vor allem auch in der Subtilität – ist, ist bekannt. (Ruf bei der ÖVP: Naja! Hält sich in Grenzen!) Aber wenn er immer davon spricht, dass nur die verschränkte Ganztagsschule eine vollwertige Schule ist – wobei ich weiß, dass die Kollegen dort hervorragende Arbeit leisten –, dann halte ich entgegen, dass alle Schu­len, unabhängig davon, ob sie in vier Stunden, in sechs Stunden, ganztägig oder wö­chentlich organisiert sind, Vollwertschulen in Österreich sind, liebe Kolleginnen und Kollegen! (Beifall bei der ÖVP und den Freiheitlichen.)

        Für uns ist die ganztägige Betreuung ein Angebot, weil wir der Entscheidungsfreiheit der Familien, auch was die Freizeitgestaltung betrifft, Rechnung tragen wollen und weil auch das Kennen-Lernen und Sich-Bewegen in anderen Gemeinschaften als der Schule – fragt einmal die Kinder! – ein notwendiges Aufrüsten im Lernprozess und im Erwachsenwerden ist. (Beifall bei der ÖVP und den Freiheitlichen.)

        Mir ist in dem Zusammenhang eine Äußerung des Vorsitzenden des Staatsschulden­ausschusses Univ.-Prof. DDr. Frisch wichtig, der eine sinnvolle Perspektive für den Stabilitätspakt ab dem Jahr 2007 gezeichnet hat, und wir haben natürlich damit zu kämpfen, dass die Länder die Lehrer-Schüler-Relation im Finanzausgleich festge­schrieben haben.

        Ich halte aber fest, dass das nicht ad libitum fortzuschreiben ist. Wir sollten den Rück­gang der Schülerzahlen, der jetzt in der Grundstufe und daher in der Verantwortung der Länder liegt, der sich dann in die Sekundarstufe I und II fortschreiben wird, dazu nützen, dass wir auch kleinere Klassen bilden können und dass wir die kleinen Schulen in den ländlichen Räumen entsprechend erhalten. Das ist ein wichtiges Anliegen mei­ner Partei. (Beifall bei der ÖVP und den Freiheitlichen.)

        Liebe Kolleginnen und Kollegen! Wir haben am ersten Tag dieses dreitägigen Plenums über das Szenario über Europa, über die Fehlzündungen im weiteren Prozess in Euro­pa, über nationale Egoismen geredet; wir sind gestern von dem Menschen verachten­den Terror überrascht worden; wir sprechen heute von Bildung: Irgendwie scheint das doch auch zusammenzugehören. Wie lange sind wir denn eigentlich von derartigen Bildern des Grauens betroffen und berührt? Wer erinnert sich denn wirklich mit Betrof­fenheit an den September 2001 in New York? Oder an den Terror vor mehr als einem Jahr in Madrid?

        Liebe Kolleginnen und Kollegen! In drei Tagen gibt es ein trauriges, zehnjähriges Jubi­läum: 8 000 bosnische Serben sind in Srebrenica nicht ums Leben gekommen, sie sind abgeschlachtet worden. Wie berührt das eigentlich noch? Was kann Schule dabei leisten?

        Und da bin ich schon bei Josef Broukal, der auch von der Werterziehung, von der Her­zensbildung neben der fachlichen Ausbildung gesprochen hat. Wir müssen unseren Kindern wieder mehr Rücksichtnahme, Verantwortung, Freundschaft statt Egoismus, Verantwortung auch für die Zukunft und eine Absage an die Zeitgeist-Gesellschaft an­erziehen, liebe Kolleginnen und Kollegen. (Beifall bei der ÖVP und den Freiheitlichen.)

        Das kann Schule nicht allein leisten. Roman Herzog, der von mir sehr geschätzte frü­here Präsident der Bundesrepublik Deutschland, hat formuliert: Immer mehr Eltern
        geben ihren Erziehungsauftrag an die Schule ab, und die soll richten, was sie zu Hause nicht schaffen.

        Wir dürfen unsere Pädagoginnen und Pädagogen nicht überfordern! Wir müssen auch für sie den notwendigen Freiraum in einer guten Schulpartnerschaft schaffen. Seien wir uns bewusst, liebe Kolleginnen und Kollegen, dass die Rückkehr zu diesen Werten eigentlich nur in der Familie geschehen kann. Familie ist nicht der Ort der Vorschriften, Familie ist der Ort der Vorbilder! Wenn Werte erlebt werden oder eben nicht, dann kann das nur dort geschehen. Familienpolitik, wie wir sie verstehen, ist daher der Akzent, der auch tatsächlich zu fördern ist.

        Wenn wir unsere Kinder zu Hoffnungsträgern im wahrsten Sinne des Wortes machen müssen – und manche der Äußerungen hier müssten Angst und Panik und eigentlich mutlos machen –, geht es nicht darum, eine fatale Ich-Gesellschaft als eine Gesell­schaftsform der Zukunft, sondern eine GmbH (Präsidentin Mag. Prammer gibt das Glockenzeichen), eine Gesellschaft mit begründeter Hoffnung, für unsere Jugend in einer friedlichen Zukunft zu bilden. (Bravorufe bei der ÖVP. – Anhaltender lebhafter Beifall bei der ÖVP und den Freiheitlichen.)
        ";

        assert!(compare_words(truth, &speech_text) > 0.96);
    }

    #[tokio::test]
    async fn test_speech_beate_schasching() {
        let speech = Speech {
            delegate_id: 8181,
            delegate_display_name: "Beate Schasching (S)".into(),
            opinion: Opinion::Infavor,
            spoken_text_html_urls: vec![SpeechTextInfo {
                site_start: Some(48),
                site_end: Some(49),
                document_url:
                    "https://www.parlament.gv.at/dokument/XXII/NRSITZ/117/SEITE_0048.html".into(),
            }],
        };

        let mut text_extractor = TextExtractor::new();
        let speech_text = text_extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();

        dbg!(&speech_text);

        let truth = "Beate Schasching (SPÖ): Frau Präsidentin! Frau Bundesministerin! Herr Staatssekretär! (Abg. Scheibner – auf die Tafel am Boden weisend –: Jetzt ist es umgefallen!) – Mein Taferl ist umgefallen? – Danke schön. (Abg. Scheibner: Man hätte vielleicht dem Kollegen Neugebauer mehr Redezeit geben sollen!)

        Damit die Zuseherinnen und Zuseher dort nachlesen können, was wir unter Ganztags­schule verstehen, und damit sie auch unsere Bildungsziele nachlesen können! (Die Rednerin stellt eine Tafel mit der Aufschrift „www.ganztagsschule.at“ vor sich auf das Rednerpult. – Abg. Dr. Sonnberger: Zwangstagsschule! Sie haben das „z“ verges­sen!)

        Ich möchte an die durchaus bemerkenswerte Rede meines Kollegen Neugebauer an­schließen (demonstrativer Beifall bei der ÖVP) und sagen, dass viele Kinder aus kriegsführenden Ländern in der Vergangenheit mit ihren Eltern nach Österreich ge­flüchtet sind und dass wir in Österreich und vor allem in Wien diese Kinder in unsere Schulen aufgenommen und integriert haben. Das haben viele Lehrerinnen und Lehrer mit enormer Anstrengung und Engagement und gegen die Widrigkeiten des Schulsys­tems, das ihnen dies erschwert hat, durchgeführt. Dafür müssen wir uns sehr herzlich bedanken. (Beifall bei der SPÖ.)

        Ich möchte aber zum heutigen Schulreformpaket kommen und vor allen Dingen fest­stellen, dass sich viele Tausende Schülerinnen und Schüler wie auch ihre Eltern, Leh­rerinnen und Lehrer ihre Ferien wohl verdient haben. Manch eine oder einer dieser Schülerinnen und Schüler hätte sich vielleicht gewünscht, mit der ganzen Familie einen Monat am Meer zu verbringen. Das geht halt nicht für jeden. Das ist ein schönes Ziel. Es ist schade, dass sich das nicht jeder leisten kann oder nicht jeder die Zeit dazu hat. Manche von ihnen müssen mit einem aufblasbaren Babyplantschbecken zu Rande kommen.

        Genauso geht es uns jetzt und heute mit dieser hier vorgelegten Reform. Wir haben uns nach PISA, nach den Ergebnissen der Zukunftsdiskussion, nach dem Fall der Zweidrittelmehrheit und einer gemeinsamen intensiven Schuldebatte gewünscht, dass es wirklich zu einer Reform kommt. Wir haben ein aufblasbares Babyplantschbecken
        bekommen: ein Minireförmchen, etwas ganz Kleines, etwas, durch das Sie versuchen, uns das zurückzugeben, was in den letzten fünf Jahren verloren gegangen ist.

        Das möchte ich jetzt im Detail ausführen, weil ich denke, dass viele Menschen, die sich wünschen, dass sich unsere Schule verbessert, und es dringend brauchen, dass sich unser Schulsystem verbessert, wissen sollen, was sie tatsächlich erwartet.

        Wir, die SPÖ, stellen uns vor, dass wir kleinere Klassen bilden. Die SPÖ stellt sich vor, dass es mehr gemeinsames Lernen geben muss. Wir stellen uns vor und wir fordern ein, dass mehr individueller Unterricht geschieht, dass über den Tag verteilt mehr pädagogische Konzepte für die Eltern und Kinder zur Verfügung stehen. Das bedeutet natürlich auch, die Form der Ganztagsschule, und zwar zu gleichen Bedingungen wie diese Nachmittagsbetreuung, einzuführen. Es ist nicht Gleichheit im Schulsystem gegeben, wenn sich 15 Eltern von Kindern für eine Nachmittagsbetreuung entscheiden dürfen und zwei Drittel einer Schule gebraucht werden, um eine Ganztagsschule einzu­führen. Das ist nicht die Gleichheit, die wir fordern. Das ist nicht die Chancengleichheit im Auswählen des Schulsystems.

        Daher können wir dem Gesetzentwurf nur ganz schweren Herzens zustimmen, weil ein paar kleine Punkte im Babyplantschbecken verwirklicht wurden, aber das ist uns zu wenig. Wir wünschen uns zumindest die eine ganze Woche am Meer. Das wäre auch im Sinne der Forderung des Dachverbands der Elternvereine, wo 90 000 Bürgerinnen und Bürger für eine Sicherung der Schulqualität unterschrieben haben: Geld allein garantiert noch nicht qualitätsvolle Bildung. Aber qualitätsvolle Bildung hat eben auch ihren Preis. – Das sagen die Elternvereine. Und daher fordern wir das in allen unseren Bildungsmaßnahmen ein. (Beifall bei der SPÖ.)

        Einen Satz zur Umbenennung von „Leibeserziehung“ in „Bewegung und Sport“ möchte ich anfügen. Fünf Jahre alt ist mein Antrag, warum ist er nie umgesetzt worden? – Weil die Regierungsparteien nicht zugestimmt haben. Jetzt ist er in der Reform enthalten. (Abg. Amon: Sie haben nicht mitgestimmt beim Paket!) Da sage ich: gut, wunderbar. – Was ist aber die Substanz dahinter? Es heißt jetzt „Bewegung und Sport“, aber es gibt weniger Bewegung und Sport in der Schule, weil viele unverbindliche Übungen beim ersten und viele Turnstunden beim zweiten Sparpaket im Bildungsbereich gekürzt wur­den.

        Was wir brauchen, ist mehr verpflichtender Turnunterricht in der Schule! (Präsidentin Mag. Prammer gibt das Glockenzeichen.) Es hilft leider nicht, Herr Staatssekretär, dass wir uns gemeinsam anstrengen, ein bisserl etwas in Kooperation zu tun. (Präsi­dentin Mag. Prammer gibt neuerlich das Glockenzeichen.) Es muss pflichtmäßig eine tägliche Bewegungsstunde in den Schulen geben. (Beifall bei der SPÖ und den Grü­nen.)
        ";
        assert!(compare_words(truth, &speech_text) > 0.96);
    }

    // XXVII/NRSITZ/62/
    #[tokio::test]
    async fn test_speeches_xxvii_62() {
        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/62")
            .await
            .unwrap();

        let mut text_extractor = TextExtractor::new();
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
    }

    #[tokio::test]
    async fn test_speeches_xxviii_57() {
        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVIII/NRSITZ/57")
            .await
            .unwrap();

        dbg!(&plenar);
    }
}
