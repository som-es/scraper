use std::collections::{BTreeMap, HashSet};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use common_scrapes::{Document, FileType, Opinion, PlenarSpeech, Speech};
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
    text_utils::remove_multiple_of,
    AustriaScraper, GegenstandPaths, Scraper, TextExtractor,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Debate {
    pub about: String,
    pub speeches: Vec<PlenarSpeech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlenarInfo {
    pub title: String,
    pub description: String,
    pub ityp: String,
    pub inr: u64,
    pub debates: Vec<Debate>,
    pub date: NaiveDateTime,
    pub call_to_orders: Vec<CallToOrderInfo>,
    pub gp: String,
    pub absences: Option<Vec<String>>,
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

        let ityp = plenar_content_0["ityp"].as_str().unwrap().to_string();
        let inr = plenar_content_0["inr"].as_u64().unwrap();

        let gp_code = plenar_content_0["gp_code"].as_str().unwrap().to_string();

        let title = plenar_content_0["title"]
            .as_str()
            .unwrap_or(&description)
            .to_string();

        let date = NaiveDateTime::parse_from_str(
            plenar_content_0["einlangen"].as_str().unwrap(),
            "%Y-%m-%dT%H:%M:%S",
        )
        .unwrap();

        let mut document_link_speeches = vec![];
        let mut call_to_orders = vec![];
        if let Some(stages) = plenar_content_0.get("stages") {
            document_link_speeches = extract_speeches_from_stages(stages).unwrap_or_default();
            call_to_orders = extract_call_to_order_info_from_stages(&ityp, inr, stages);
        }

        let past_debates = if let Some(content_1) = raw_plenar["content"].as_array().unwrap().get(1)
        {
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

        let documents = match documents(&raw_documents) {
            Some(d) => d,
            None => {
                vec![]
            }
        };

        let absences = extract_absences(&documents).await;

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
            gp: gp_code,
            absences,
        })
    }
}

async fn extract_absences(documents: &[Document]) -> Option<Vec<String>> {
    for document in documents {
        println!("doc: {document:?}");
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
    extract_absences_from_text(&text)
}

fn extract_absences_from_text(text: &str) -> Option<Vec<String>> {
    // remove this later -> only for testing purposes
    if text.len() < 50000 {
        return None;
    }
    let absence_marker_regex = Regex::new(r"verhindert\s*gemeldet").unwrap();
    let absence_match = absence_marker_regex.find(text)?;

    let mut start_idx = absence_match.start();

    while text.as_bytes()[start_idx] != b'\n' {
        start_idx -= 1;
    }

    // let print = &text[start_idx..start_idx + 400];
    // println!("print: {print}");

    let open_ended_absence = &text[start_idx..].replace('\u{a0}', " ");
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
        .min(open_ended_absence.find(", die ich").unwrap_or(usize::MAX))];

    println!(
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
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    let info = info
        .split(',')
        .into_iter()
        .map(|x| x.split("und"))
        .flatten()
        .map(|x| x.split("sowie"))
        .flatten()
        .map(|x| {
            x.split_inclusive(char::is_whitespace)
                // .inspect(|x| println!("pre x: {x:?}, {}", x.chars().filter(|x| x.is_alphabetic()).count()))
                .filter(|split| {
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
            // .inspect(|x| println!("after x: {x:?}"))
        })
        .map(|x| x.collect::<String>())
        // .inspect(|x| println!("x: {x:?}"))
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
    debates_with_speeches: Vec<(String, Vec<PlenarSpeech>)>,
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
    for (debates_key, about, mut debate_speeches) in debates_with_speeches_keys {
        let document_link_speeches = document_link_speeches
            .get(&debates_key)
            .map(|speeches| speeches.as_slice())
            .unwrap_or_default();
        for (debate_speech, document_link_speech) in
            debate_speeches.iter_mut().zip(document_link_speeches)
        {
            debate_speech.speech = document_link_speech.clone();
        }
        debates.push(Debate {
            about,
            speeches: debate_speeches,
        });
    }
    debates
}

pub fn extract_speeches(past_debates: &[Value]) -> Vec<(String, Vec<PlenarSpeech>)> {
    let mut debates = vec![];

    past_debates
        .iter()
        .flat_map(|past_debate| {
            Some((
                past_debate["text"].as_str()?,
                past_debate["speeches"].as_array()?,
            ))
        })
        .filter(|(_, speeches)| !speeches.is_empty())
        .flat_map(|(text, speeches)| {
            debates.push(text.to_string());
            Some((
                text.to_string(),
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
    use common_scrapes::{Document, Opinion};

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
    async fn test_look_all_plenar_gegenstand_paths() {
        let mut scraper = AustriaScraper::default();

        for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let plenar_infos = scraper.post_plenar_payload().await.unwrap();

            let paths = scraper
                .plenar_paths(plenar_infos)
                .expect("Failed to get paths");
            for path in paths {
                println!("path: {path}");
                // path with no content at 1: /gegenstand/XXVII/NRSITZ/216

                // newer ones do not provide a document

                let plenar_info = scraper.get_plenar(&path).await.unwrap();
                println!("plenar_info: {:?}", plenar_info.absences);
            }
        }
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
                "Reinhold\rLopatka",
                "Petra Bayr",
                "Julia Elisabeth",
                "Klaus Köchl",
                "Michael Seemayer",
                "Petra Tanzler",
                "Hubert Fuchs",
                "Herbert\rKickl",
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
    }
    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_19() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/19")
            .await
            .unwrap();

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
        assert_eq!(plenar.debates[0].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
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
    async fn test_get_plenar_gegenstand_xxii_nrsitz_117() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXII/NRSITZ/117")
            .await
            .unwrap();
        let mut extractor = TextExtractor::new(&AustriaScraper::BASE_URL);

        let mut lengths = vec![];
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                let speech_info = speech.speech.spoken_text_html_urls.first().unwrap();
                let speech_text = extractor
                    .speech_extraction_with_fit(&speech_info, &speech.speech.delegate_display_name)
                    .await
                    .unwrap();
                lengths.push(speech_text.len());
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
        assert_eq!(opinion_none_speech.speech.opinion, Opinion::None);
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

        let mut extractor = TextExtractor::new(&AustriaScraper::BASE_URL);

        let mut lengths = vec![];
        for debate in &plenar.debates {
            for speech in &debate.speeches {
                let speech_info = speech.speech.spoken_text_html_urls.first().unwrap();
                let speech_text = extractor
                    .speech_extraction_with_fit(&speech_info, &speech.speech.delegate_display_name)
                    .await
                    .unwrap();
                lengths.push(speech_text.len());
                // println!("-------------------------------------- {}{}", AustriaScraper::BASE_URL, speech_info.document_url);
                // println!("speech_text: {speech_text}");
                // println!("--------------------------------------");
            }
        }

        let required_lenghts = [
            10832usize, 5661, 3878, 8295, 11226, 9298, 8087, 3890, 4564, 4038, 4950, 4216, 3592,
            2600, 2739, 1902, 2150, 6368, 4873, 4830, 5002, 6103, 9346, 4242, 7348, 2641, 3348,
            2743, 2917, 3964, 440, 3532, 2464, 3945, 2896, 6627, 5222, 4424, 2266, 2860, 3043,
            2953, 4623, 3139, 4498, 2449, 1973, 1536, 5548, 2939, 2135, 1875, 4010, 3828, 18815,
            5729, 13164, 7177, 4682, 4641, 6584, 5366, 5089, 3362, 3956, 5698, 6243, 4292, 5468,
            4922, 6495, 5364, 4746, 8129, 4569, 5283, 4702, 7174, 3447, 4147, 4492, 5104, 1315,
            4355, 5224, 3165, 3388, 4848, 1631, 2517, 3096, 2989, 2428, 3643, 3599, 1893, 2878,
            2712, 2562, 5737, 6611, 3943, 4050, 1812, 1960, 3379, 2407, 3243, 3065, 3699, 3102,
            6188, 4963, 2536, 1856, 6821, 7616, 3209, 3915, 2276, 2855, 4114, 3718, 4115, 3974,
            2238, 1284, 3041, 1661, 4066, 3815, 2966, 3395, 2212, 2464, 4087, 3612, 3172, 2496,
            4265, 4285, 3061, 2578, 3222, 3161, 1860, 2780, 3097, 3123, 2990, 3324, 3200, 255,
            5484, 3103, 2784, 2724, 4508, 2985, 1193, 1984, 8285, 7088, 4518, 5759, 3436, 2327,
            2138, 5425, 1950, 1265, 1420, 1068,
        ];
        assert_eq!(&required_lenghts, lengths.as_slice());
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

            let Some(debates) = plenar.debates.get(0) else {
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
    async fn test_absences_extraction_from_doc() {
        let link = "/dokument/XXIII/NRSITZ/72/fnameorig_144783.html".to_string();
        let doc = Document {
            title: "nix".into(),
            link,
            ftype: common_scrapes::FileType::HTML,
        };

        let absences = extract_absences_from_document(&doc).await.unwrap();
        println!("absences: {absences:?}");
    }
}
