use async_trait::async_trait;
use common_scrapes::{Opinion, PlenarSpeech, Speech};
use serde_json::{json, Value};

use crate::{
    error::ScraperError, speeches::extract_speeches_from_stages, AustriaScraper, GegenstandPaths,
    Scraper,
};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct Debate {
    about: String,
    speeches: Vec<PlenarSpeech>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct PlenarInfo {
    description: String,
    ityp: String,
    inr: u64,
    debates: Vec<Debate>,
}

#[async_trait]
pub trait Plenar: Scraper {
    const FILTER_PATH: &'static str;
    fn plenar_payload(&self) -> Self::Payload;

    async fn post_plenar_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.plenar_payload();
        self.post_payload(
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
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

        let mut document_link_speeches = vec![];
        if let Some(stages) = plenar_content_0.get("stages") {
            extract_speeches_from_stages(stages, &mut document_link_speeches);
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

        let debate_with_speeches = extract_speeches(past_debates);

        let debates = combine_speech_info(debate_with_speeches, &document_link_speeches);

        Some(PlenarInfo {
            description,
            ityp,
            inr,
            debates,
        })
    }
}

fn combine_speech_info(
    debate_with_speeches: Vec<(String, Vec<PlenarSpeech>)>,
    document_link_speeches: &[Speech],
) -> Vec<Debate> {
    let mut end_idx = 0;
    let mut debates = vec![];
    for (debate_title, mut speeches) in debate_with_speeches {
        let speeches_len = speeches.len();
        for (speech, doc_link_speech) in speeches.iter_mut().zip(
            document_link_speeches[end_idx..end_idx + speeches_len]
                .iter()
                .cloned(),
        ) {
            speech.spoken_text_html_urls = doc_link_speech.spoken_text_html_urls;
            speech.opinion = doc_link_speech.opinion
        }
        end_idx += speeches.len();

        debates.push(Debate {
            about: debate_title,
            speeches,
        })
    }
    debates
}

pub fn extract_speeches(past_debates: &[Value]) -> Vec<(String, Vec<PlenarSpeech>)> {
    let mut debates = vec![];

    past_debates
        .into_iter()
        .flat_map(|past_debate| {
            Some((
                past_debate["text"].as_str()?,
                past_debate["speeches"].as_array()?,
            ))
        })
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

        assert_eq!(plenar.debates[1].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        assert_eq!(
            plenar.debates[1].speeches[0].spoken_text_html_urls[0],
            "/dokument/XXVII/NRSITZ/211/A_-_12_03_00_00295752.html"
        );
        assert_eq!(plenar.debates[1].speeches[0].duration_in_seconds, 831);
        assert_eq!(
            plenar.debates[2].speeches[plenar.debates[2].speeches.len() - 4].duration_in_seconds,
            300
        );
        assert_eq!(
            plenar.debates[2].speeches[plenar.debates[2].speeches.len() - 4].spoken_text_html_urls
                [0],
            "/dokument/XXVII/NRSITZ/211/A_-_15_24_25_00295995.html"
        );
    }

    #[tokio::test]
    async fn test_get_plenar_gegenstand_xxvii_nrsitz_117() {
        let scraper = AustriaScraper::default();

        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();

        // assert_eq!(plenar.debates[1].about, "Dringlicher Antrag der Abg. Rendi-Wagner betr. Totalversagen der Bundesregierung im Kampf gegen die Teuerung!");
        let first_speech = &plenar.debates[1].speeches[0];
        assert_eq!(first_speech.delegate_id, 5642);
        assert_eq!(first_speech.opinion, Opinion::Against);

        let opinion_none_speech = &plenar.debates[1].speeches[2];
        assert_eq!(opinion_none_speech.delegate_id, 83127);
        assert_eq!(opinion_none_speech.opinion, Opinion::None);
        assert_eq!(opinion_none_speech.duration_in_seconds, 5 * 60 + 12);

        let next_debate_speech = &plenar.debates[2].speeches[1];
        assert_eq!(next_debate_speech.delegate_id, 2006);
        assert_eq!(next_debate_speech.opinion, Opinion::Infavor);
        assert_eq!(next_debate_speech.duration_in_seconds, 5 * 60 + 32);

        let last_speech_of_next_debate = plenar.debates[2].speeches.last().unwrap();
        assert_eq!(last_speech_of_next_debate.delegate_id, 83299);
        assert_eq!(last_speech_of_next_debate.opinion, Opinion::Infavor);
        assert_eq!(last_speech_of_next_debate.duration_in_seconds, 2 * 60 + 20);

        let next_debate_speech = &plenar.debates[3].speeches[0];
        assert_eq!(next_debate_speech.delegate_id, 8178);
        assert_eq!(next_debate_speech.opinion, Opinion::Against);
        assert_eq!(next_debate_speech.duration_in_seconds, 4 * 60 + 28);
    }
}
