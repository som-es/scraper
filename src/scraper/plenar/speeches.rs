use crate::plenar::{Debate, DebateInfo};
use common_scrapes::{Opinion, PlenarSpeech, Reference, Speech};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn combine_speech_info(
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

pub(super) fn extract_speeches(past_debates: &[Value]) -> Vec<(DebateInfo, Vec<PlenarSpeech>)> {
    past_debates
        .iter()
        .flat_map(|past_debate| {
            dbg!(past_debate);
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
