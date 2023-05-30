use common_scrapes::Speech;
use serde_json::Value;

fn extract_spoken_text_html_urls(rede: &Value) -> Vec<String> {
    rede.get(2)
        .map(|spoken_text_infos| {
            spoken_text_infos
                .as_array()
                .unwrap_or(&vec![])
                .iter()
                .flat_map(|info| Some(info["url"].as_str()?.to_string()))
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

pub fn extract_speeches_from_reden(rows_reden: &Value, speeches: &mut Vec<Speech>) -> Option<()> {
    for speech in rows_reden["data"]["rows"].as_array()? {
        let Some(speech) = extract_speech(speech) else {
            continue;
        };
        speeches.push(speech);
    }
    Some(())
}
pub fn extract_speeches_from_stages(stages: &Value) {}

pub fn extract_speeches(legis_content: &Value) -> Option<Vec<Speech>> {
    let mut speeches = vec![];
    for phase in legis_content.get("phase")?.as_array()? {
        let Some(stages) = phase.get("stages") else {
            continue;
        };

        for stage in stages.as_array()? {
            let Some(reden) = stage.get("reden") else {
                continue;
            };

            extract_speeches_from_reden(reden, &mut speeches)?;
        }
    }
    Some(speeches)
}

pub fn extract_speech(speech: &Value) -> Option<Speech> {
    let spoken_text_html_urls = extract_spoken_text_html_urls(speech);

    let a_tag_delegate_url = speech[0].as_str()?;
    let delegate_id = extract_person_id_from_a_tag(a_tag_delegate_url)?;

    let opinion = speech[1].as_str()?;

    let infavor = match opinion {
        "Pro" => Some(true),
        "Contra" => Some(false),
        _ => None,
    };

    Some(Speech {
        delegate_id,
        infavor,
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
