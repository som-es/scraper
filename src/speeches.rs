use common_scrapes::{Opinion, Speech, SpeechTextInfo};
use serde_json::Value;

use crate::extract_text_from_html;

fn extract_spoken_text_html_urls(rede: &Value) -> Vec<SpeechTextInfo> {
    rede.get(2)
        .map(|spoken_text_infos| {
            spoken_text_infos
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .flat_map(|info| {
                    Some(SpeechTextInfo {
                        site_start: info["fund_von"].as_u64(),
                        site_end: info["fund_bis"].as_u64(),
                        document_url: info["url"].as_str()?.to_string(),
                    })
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default()
}

fn extract_person_id_from_a_tag(a_tag_delegate_url: &str) -> Option<u64> {
    a_tag_delegate_url
        .split('/')
        .nth(2)
        .and_then(|id| id.split('"').next()?.parse::<u64>().ok())
}

pub fn extract_speeches_from_reden(rows_reden: &Value) -> Option<Vec<Speech>> {
    Some(
        rows_reden["data"]["rows"]
            .as_array()?
            .iter()
            .flat_map(extract_speech)
            .collect(),
    )
}
pub fn extract_speeches_from_stages(stages: &Value) -> Option<Vec<Vec<Speech>>> {
    Some(
        stages
            .as_array()?
            .iter()
            .flat_map(|stage| stage.get("reden"))
            .flat_map(extract_speeches_from_reden)
            .collect(),
    )
}

pub fn extract_speeches(legis_content: &Value, only_nr: bool) -> Option<Vec<Speech>> {
    Some(
        legis_content
            .get("phase")?
            .as_array()?
            .iter()
            .filter(|phase| {
                if only_nr {
                    phase["name"]
                        .as_str()
                        .unwrap()
                        .contains("Plenarberatungen NR")
                } else {
                    true
                }
            })
            .flat_map(|phase| phase.get("stages"))
            .flat_map(extract_speeches_from_stages)
            .flatten()
            .flatten()
            .collect(),
    )
}

pub fn extract_speech(speech: &Value) -> Option<Speech> {
    let spoken_text_html_urls = extract_spoken_text_html_urls(speech);

    // println!("speech: {}", serde_json::to_string_pretty(speech).unwrap());

    let a_tag_delegate_url = speech[0].as_str()?;
    let delegate_id = extract_person_id_from_a_tag(a_tag_delegate_url)?;
    let delegate_display_name = extract_text_from_html(a_tag_delegate_url);

    let opinion = speech[1].as_str().unwrap_or("None");

    let opinion = match opinion {
        "Pro" => Opinion::Infavor,
        "Contra" => Opinion::Against,
        "None" => Opinion::None,
        other => Opinion::Other(other.into()),
    };

    Some(Speech {
        delegate_id,
        delegate_display_name,
        opinion,
        spoken_text_html_urls,
    })
}

#[cfg(test)]
mod tests {
    use common_scrapes::SpeechTextInfo;

    use crate::{AustriaScraper, Scraper, TextExtractor};

    use super::extract_person_id_from_a_tag;

    #[test]
    fn test_extract_id_from_url() {
        let id = extract_person_id_from_a_tag(
            r#"<a href="/person/52687" target="_blank">Alois Stöger, diplômé (S)</a>"#,
        );
        assert_eq!(id, Some(52687))
    }

    #[tokio::test]
    async fn test_speech_missing_text1() {
        // fixed
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        let speech_info = SpeechTextInfo {
            site_start: Some(36),
            site_end: Some(38),
            document_url: "/dokument/XXV/NRSITZ/94/fnameorig_525316.html#Seite_0036.html".into(),
        };
        let text = extractor
            .speech_extraction_with_fit(&speech_info, "Mag. Elisabeth Grossmann (S)")
            .await
            .unwrap();

        let len = text.len();
        println!("text: {text}, len: {len}");
        assert_eq!(len, 5220);
    }

    //
    #[tokio::test]
    async fn test_speech_missing_text2() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        let speech_info = SpeechTextInfo {
            site_start: Some(270),
            site_end: None,
            document_url: "/dokument/XXVII/NRSITZ/89/fnameorig_981234.html#Seite_0270.html".into(),
        };
        let text = extractor
            .speech_extraction_with_fit(&speech_info, "Erwin Angerer (F)")
            .await
            .unwrap();
        let len = text.len();
        assert_eq!(len, 412);
        println!("text: {text}, len: {len}");
    }

    #[tokio::test]
    async fn test_speech_missing_text3() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let speech_info = SpeechTextInfo {
            site_start: Some(16),
            site_end: Some(18),
            document_url: "/dokument/XXV/NRSITZ/125/fnameorig_562456.html#Seite_0016.html".into(),
        };

        let text = extractor
            .speech_extraction_with_fit(&speech_info, "Mag. Dr. Matthias Strolz (N)")
            .await
            .unwrap();

        let len = text.len();
        assert_eq!(len, 4846);
        // println!("text: {text}, len: {len}");
    }
    // Mag. Dr. Matthias Strolz (N)
    
    #[tokio::test]
    async fn test_speech_missing_text4() {
        std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let speech_info = SpeechTextInfo {
            site_start: None,
            site_end: None,
            document_url: "/dokument/XXVII/NRSITZ/268/A_-_12_54_45_00326501.html".into(),
        };

        let text = extractor
            .speech_extraction_with_fit(&speech_info, " Angela Baumgartner (V)")
            .await
            .unwrap();

        let len = text.len();
        println!("text: {text}, len: {len}");
        assert_eq!(len, 4740);
    }
}
