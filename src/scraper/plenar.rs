use std::collections::{BTreeMap, HashMap};

use async_trait::async_trait;
use chrono::NaiveDateTime;
use common_scrapes::{Opinion, PlenarSpeech, Speech};
use serde_json::{json, Value};

use crate::{
    call_to_order::{
        extract_call_to_order_id_from_path, extract_call_to_order_paths_from_stages,
        CallToOrderInfo,
    },
    error::ScraperError,
    person_url_to_id,
    speeches::extract_speeches_from_stages,
    AustriaScraper, GegenstandPaths, Scraper,
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
    pub legislative_period: String,
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

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo>;

    async fn get_plenar(&self, plenar_path: &str) -> Result<PlenarInfo, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(plenar_path).await?;
        Self::raw_plenar_into_plenar_info(raw_proposal)
            .ok_or(ScraperError::PlenarGegenstandParseError)
    }
}

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

    fn raw_plenar_into_plenar_info(raw_plenar: Value) -> Option<PlenarInfo> {
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
            legislative_period: gp_code,
        })
    }
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

#[macro_export]
macro_rules! key_from_speeches {
    ($speeches:ident) => {
        $speeches
            .iter()
            .map(|speech| speech.delegate_id)
            .collect::<Vec<u64>>()
    };
}

fn combine_speech_info(
    debates_with_speeches: Vec<(String, Vec<PlenarSpeech>)>,
    document_link_speeches: Vec<Vec<Speech>>,
) -> Vec<Debate> {
    let document_link_speeches = document_link_speeches
        .into_iter()
        .map(|speeches| (crate::key_from_speeches!(speeches), speeches))
        .collect::<BTreeMap<_, _>>();

    let debates_with_speeches_keys = debates_with_speeches
        .into_iter()
        .map(|(about, speeches)| (crate::key_from_speeches!(speeches), about, speeches))
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
            debate_speech.opinion = document_link_speech.opinion.clone();
            debate_speech.spoken_text_html_urls =
                document_link_speech.spoken_text_html_urls.clone();
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
                            delegate_id,
                            ty,
                            duration_in_seconds,
                            // TODO: get spoken text html urls, use content[0]
                            spoken_text_html_urls: vec![],
                            opinion: Opinion::None,
                        })
                    })
                    .collect::<Vec<_>>(),
            ))
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use common_scrapes::Opinion;

    use crate::AustriaScraper;

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
        let scraper = AustriaScraper::default();

        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        for path in paths {
            println!("path: {path}");
            // path with no content at 1: /gegenstand/XXVII/NRSITZ/216
            let plenar_info = scraper.get_plenar(&path).await.unwrap();
            println!("plenar_info: {plenar_info:?}");
        }
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_211() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/211")
            .await
            .unwrap();

        assert_eq!(plenar.legislative_period, "XXVII");
        assert_eq!(plenar.ityp, "NRSITZ");
        assert_eq!(plenar.inr, 211);
        assert_eq!(plenar.debates[0].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        assert_eq!(
            plenar.debates[0].speeches[0].spoken_text_html_urls[0],
            "/dokument/XXVII/NRSITZ/211/A_-_12_03_00_00295752.html"
        );
        assert_eq!(plenar.debates[0].speeches[0].duration_in_seconds, 831);
        assert_eq!(
            plenar.debates[1].speeches[plenar.debates[1].speeches.len() - 4].duration_in_seconds,
            300
        );
        assert_eq!(
            plenar.debates[1].speeches[plenar.debates[1].speeches.len() - 4].spoken_text_html_urls
                [0],
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
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_117() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();

        // assert_eq!(plenar.debates[1].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        let first_speech = &plenar.debates[0].speeches[0];
        assert_eq!(first_speech.delegate_id, 5642);
        assert_eq!(first_speech.opinion, Opinion::Against);

        let opinion_none_speech = &plenar.debates[0].speeches[2];
        assert_eq!(opinion_none_speech.delegate_id, 83127);
        assert_eq!(opinion_none_speech.opinion, Opinion::None);
        assert_eq!(opinion_none_speech.duration_in_seconds, 5 * 60 + 12);

        let next_debate_speech = &plenar.debates[1].speeches[1];
        assert_eq!(next_debate_speech.delegate_id, 2006);
        assert_eq!(next_debate_speech.opinion, Opinion::Infavor);
        assert_eq!(next_debate_speech.duration_in_seconds, 5 * 60 + 32);

        let last_speech_of_next_debate = plenar.debates[1].speeches.last().unwrap();
        assert_eq!(last_speech_of_next_debate.delegate_id, 83299);
        assert_eq!(last_speech_of_next_debate.opinion, Opinion::Infavor);
        assert_eq!(last_speech_of_next_debate.duration_in_seconds, 2 * 60 + 20);

        let next_debate_speech = &plenar.debates[2].speeches[0];
        assert_eq!(next_debate_speech.delegate_id, 8178);
        assert_eq!(next_debate_speech.opinion, Opinion::Against);
        assert_eq!(next_debate_speech.duration_in_seconds, 4 * 60 + 28);
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
                if speech.delegate_id != 72959 {
                    continue;
                }
                // let text = extractor.extract_text_from_url(&speech).await;
                // println!("{}", text);
            }
        }
    }
}
