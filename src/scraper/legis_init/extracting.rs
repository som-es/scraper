use std::collections::HashSet;

use common_scrapes::{AcceptedState, Party, Reference, Vote};
use serde_json::Value;

use crate::{
    extract_reference_from_gegenstand_url, extract_reference_from_gegenstand_url_text,
    filter::filter_name_extraction,
};

pub fn extract_references(legis_content: &Value) -> Option<Vec<Reference>> {
    Some(
        legis_content["reference"]
            .as_array()?
            .iter()
            .flat_map(|v| extract_reference_from_gegenstand_url(&v["url"].as_str()?[2..]))
            .collect::<Vec<_>>(),
    )
}

pub fn is_accepted_based_on_votes(
    vote_sum: usize,
    pro_count: usize,
    requires_simple_majority: bool,
) -> AcceptedState {
    let pro_share = pro_count as f64 / vote_sum as f64;
    if requires_simple_majority && pro_share >= 0.5 {
        AcceptedState::Accepted
    } else if !requires_simple_majority && pro_share >= 2_f64 / 3_f64 {
        AcceptedState::Accepted
    } else {
        AcceptedState::Declined
    }
}

pub fn extract_invisibly_declined_from_text(text: &str) -> (Option<bool>, Option<Reference>) {
    let url = extract_reference_from_gegenstand_url_text(text);
    if text.contains("abgelehnt") {
        return (Some(true), url);
    }

    if text.contains("angenommen") || text.contains("Nationalrates: Genehmigung") {
        return (Some(false), url);
    }

    (None, None)
}

pub fn extract_invisibly_declined(legis_content: &Value) -> (Option<bool>, Option<Reference>) {
    let status_description = legis_content["status"]["description"].as_str();

    let Some(status_description) = status_description else {
        return (None, None);
    };
    extract_invisibly_declined_from_text(status_description)
}

pub fn extract_plenary_date<'a>(legis_content: &'a Value, council: &str) -> Option<&'a str> {
    let mut most_fitting_date = None;
    if legis_content.get("phase").is_none() {
        if let Some(stages) = legis_content.get("stages") {
            most_fitting_date = extract_most_fitting_date(stages)
        }
        return most_fitting_date;
    }
    for phase in legis_content.get("phase")?.as_array()? {
        let Some(stages) = phase.get("stages") else {
            continue;
        };

        if !phase["name"].as_str()?.contains(council) {
            continue;
        }

        // extract_speeches_from_stages(stages, &mut speeches);
        most_fitting_date = extract_most_fitting_date(stages);
    }
    most_fitting_date
}

pub fn group_names_from_vote_section(names: &str) -> Vec<String> {
    filter_name_extraction(names.split([',', ';'])).collect()
}

pub fn extract_majority_on_indicators(legis_content: &Value) -> Option<bool> {
    let mut is_simple_majority = true;

    'outer: for phase in legis_content.get("phase")?.as_array()?.iter() {
        let Some(stages) = phase.get("stages") else {
            continue;
        };
        for stage in stages.as_array()? {
            if let Some(text) = stage.get("text") {
                if text.as_str()?.contains("Mehrheit von zwei Dritteln")
                    || text.as_str()?.contains("Bundesverfassungsgesetz")
                {
                    is_simple_majority = false;
                    break 'outer;
                }
            }
        }
    }

    if legis_content["status"]["description"]
        .as_str()?
        .contains("nicht die erforderliche 2/3 Mehrheit")
    {
        is_simple_majority = false
    }

    Some(is_simple_majority)
}

pub fn extract_most_fitting_date(raw_stages: &Value) -> Option<&str> {
    let mut fitting_date = None;
    for raw_stage in raw_stages.as_array()? {
        if raw_stage.get("priority").is_none() {
            continue;
        }
        let Some(date) = raw_stage["date"].as_str() else {
            continue;
        };
        fitting_date = Some(date)
    }
    fitting_date
}

pub fn extract_topics(legis_content: &Value, topics_name: &str) -> Option<Vec<String>> {
    let mut already_seen_topics = HashSet::new();
    Some(
        legis_content[topics_name]["data"]["bubbles"]
            .as_array()?
            .iter()
            .flat_map(|topic| topic["label"].as_str().map(|topic| topic.to_string()))
            .filter(|topic| already_seen_topics.insert(topic.clone())) // some topics are duplicated ... -> e.g. XXVII/I/2451
            .into_iter()
            .collect::<Vec<String>>(),
    )
}

pub fn extract_votes(legis_content: &Value) -> Option<Vec<Vote>> {
    Some(
        legis_content["vote"]["result"]
            .as_array()?
            .iter()
            .flat_map(|vote| {
                Some(Vote {
                    party: Party {
                        name: vote["text"].as_str()?.to_string(),
                        color: vote["color"].as_str()?.to_string(),
                        fraction: vote["fraction"].as_u64()? as u32,
                        code: vote["code"].as_str()?.to_string(),
                    },
                    infavor: vote["infavor"].as_bool()?,
                })
            })
            .collect::<Vec<Vote>>(),
    )
}
