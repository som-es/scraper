use common_scrapes::{Opinion, Speech, SpeechTextInfo};
use serde_json::Value;

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

    println!("speech: {}", serde_json::to_string_pretty(speech).unwrap());

    let a_tag_delegate_url = speech[0].as_str()?;
    let delegate_id = extract_person_id_from_a_tag(a_tag_delegate_url)?;

    let opinion = speech[1].as_str().unwrap_or("None");

    let opinion = match opinion {
        "Pro" => Opinion::Infavor,
        "Contra" => Opinion::Against,
        "None" => Opinion::None,
        other => Opinion::Other(other.into()),
    };

    Some(Speech {
        delegate_id,
        opinion,
        spoken_text_html_urls,
    })
}

#[cfg(test)]
mod tests {
    use super::extract_person_id_from_a_tag;

    #[test]
    fn test_extract_id_from_url() {
        let id = extract_person_id_from_a_tag(
            r#"<a href="/person/52687" target="_blank">Alois Stöger, diplômé (S)</a>"#,
        );
        assert_eq!(id, Some(52687))
    }
}
