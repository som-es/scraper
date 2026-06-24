use chrono::NaiveDate;
use common_scrapes::{HiddenVotes, ProContra, Stage, VoteStage, VoteType, VotedByName};
use regex::Regex;
use serde_json::Value;

use crate::{
    add_base_url_to_url, caching::CacheConfig, extract_pro_contra_party_codes_from_string,
    find_optimal_votes_by_name, legis_init::group_names_from_vote_section,
    yes_no_votes_inside_regex, yes_no_votes_prefixed_regex, yes_no_votes_regex, AustriaScraper,
    Scraper, TextExtractor,
};

use super::extract_invisibly_declined_from_text;

pub async fn extract_hidden_votes(
    legis_content: &Value,
    stop_at_phase: &str,
    cache_config: &CacheConfig,
) -> (Option<u64>, Option<HiddenVotes>) {
    let mut most_fitting_votes = (None, None);
    let status = legis_content.get("status");

    if legis_content.get("phase").is_none() {
        if let Some(stages) = legis_content.get("stages") {
            most_fitting_votes = extract_most_fitting_votes(stages, status, cache_config).await;
        }

        return most_fitting_votes;
    }

    for phase in legis_content
        .get("phase")
        .unwrap()
        .as_array()
        .unwrap()
        .iter()
        .rev()
    {
        let Some(stages) = phase.get("stages") else {
            continue;
        };

        if phase["name"].as_str().unwrap().contains(stop_at_phase) {
            most_fitting_votes = extract_most_fitting_votes(stages, status, cache_config).await;
            break;
        }
    }
    most_fitting_votes
}

pub fn classify_vote_stage(text: &str) -> Option<VoteStage> {
    let mut vote_stage = VoteStage::default();
    vote_stage.stage = if text.contains("Gesetzesvorschlag in zweiter Lesung") {
        Stage::Second
    } else if text.contains("Gesetzesvorschlag in dritter Lesung") {
        Stage::Third
    } else if text.contains("erster Lesung") {
        Stage::First
    } else {
        Stage::None
    };
    vote_stage.vote_type = if text.contains("Antrag, den ablehnenden Ausschussbericht") {
        VoteType::ReversingProposal
    } else if text.contains("Sitzung des Nationalrates")
        && text.contains("Genehmigung")
        && text.to_lowercase().contains("namentliche")
    {
        VoteType::OnlyVotedByName
    } else if text.to_lowercase().contains("namentliche abstimmung") {
        VoteType::AnyNamedVote
    } else if text.to_lowercase().contains("antrag") {
        VoteType::Proposal
    // https://www.parlament.gv.at/gegenstand/XXVII/III/835
    } else if text.contains("Antrag auf Kenntnisnahme") {
        VoteType::Proposal
    } else if text.to_lowercase().contains("gesetzesvorschlag") {
        VoteType::Proposal
    } else if text.contains("Nationalrates: Genehmigung") {
        VoteType::Proposal
    } else {
        // need to find fitting type, some stages are bs
        if vote_stage.stage != Stage::None {
            VoteType::None
        } else {
            return None;
        }
    };
    Some(vote_stage)
}

#[inline]
pub fn contains_multiple_of(text: &str, of: &str) -> bool {
    text.find(of) != text.rfind(of)
}

//https://www.parlament.gv.at/gegenstand/BR/BRSITZ/751
pub fn contains_session_or_no_links(text: &str) -> bool {
    (text.contains("gegenstand/") && !contains_multiple_of(text, "gegenstand/"))
        && (text.contains("NRSITZ/") || text.contains("gegenstand/BR/BRSITZ"))
        || !text.contains("gegenstand/")
}

pub async fn extract_most_fitting_votes(
    stages: &Value,
    status: Option<&Value>,
    cache_config: &CacheConfig,
) -> (Option<u64>, Option<HiddenVotes>) {
    let sitzung_id_regex = Regex::new(r"(\d+). Sitzung").unwrap();
    let is_site_stored_regex = Regex::new(r"SEITE_\d+").unwrap();

    let mut hidden_votes = HiddenVotes::default();
    hidden_votes.vote_stage = status
        .and_then(|status| status["description"].as_str())
        .and_then(|desc| {
            let desc = &desc[desc.find("Nationalrat").unwrap_or_default()..];
            classify_vote_stage(desc)
        });

    let mut plenary_session = None;

    for stage in stages.as_array().unwrap().iter().rev() {
        let fsth = stage["fsth"].as_array().cloned().unwrap_or_default();

        let Some(text) = stage["text"].as_str() else {
            continue;
        };

        if !(text.contains("Abstimmung")
            || text.contains("abgelehnt")
            || text.contains("angenommen")
            || (text.contains("Nationalrates: Genehmigung")))
            || ((text.to_lowercase().contains("ausschuss") || text.to_lowercase().contains("ausschuß")) && !text.contains("Nationalrat")) // auschuß: gp XX had these a lot e.g.  XX/I/308
            // || (contains_multiple_of(text, "gegenstand/") // better way would probably be to check for NRSITZ gegenstand link, if not present AND another gegenstand link is present => skip (Either zero links or a single NRSITZ link should be allowed) See: XXVII/I/604
            || (!contains_session_or_no_links(text) && !text.contains("/E/"))
        // AEA: what to do in the case of XXVIII/I/318?
        {
            continue;
        }

        // extract fitting sitzung id
        for protocol_link_obj in fsth.iter() {
            match protocol_link_obj["sitzung_id"].as_u64() {
                Some(sitzung_id_inner) => {
                    plenary_session = Some(sitzung_id_inner);
                    break;
                }
                None => continue,
            }
        }

        if plenary_session.is_none() {
            if let Some(captures) = sitzung_id_regex.captures(text) {
                if let Some(capture) = captures.get(1) {
                    plenary_session = Some(capture.as_str().parse::<u64>().unwrap())
                }
            }
        }
        let Some(vote_stage) = classify_vote_stage(text) else {
            continue;
        };
        if let Some(status_vote_stage) = hidden_votes.vote_stage {
            if status_vote_stage.vote_type.is_proposal() != vote_stage.vote_type.is_proposal() {
                continue;
            }
        }

        hidden_votes.vote_stage = Some(vote_stage);
        let (was_declined, _) = extract_invisibly_declined_from_text(text);
        hidden_votes.was_declined = was_declined;

        hidden_votes.voted_at_date = stage["date"].as_str().and_then(|date_str| {
            NaiveDate::parse_from_str(date_str, "%d.%m.%Y ")
                .ok()
                .or_else(|| {
                    log::error!("could not parse date from string: {date_str:?}");
                    None
                })
        });

        if text.to_lowercase().contains("namentlich") {
            let Some((vote_sum, pro_count, contra_count, invalid_count)) = extract_vote_count(text)
            else {
                log::error!("could not extract vote sum, pro count, etc from text: {text:?}");
                continue;
            };
            let mut delegate_votes = None;
            for protocol_link_obj in fsth.iter() {
                let Some(url) = protocol_link_obj["url"].as_str() else {
                    continue;
                };

                let url = add_base_url_to_url(url, AustriaScraper::BASE_URL);

                let mut extractor = TextExtractor::new(cache_config.clone());
                // dbg!(protocol_link_obj);

                let html_text = if is_site_stored_regex.is_match(&url.to_ascii_uppercase()) {
                    let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '/').unwrap();

                    let Some(site_start) = protocol_link_obj["fund_von"].as_u64() else {
                        continue;
                    };

                    let site_end = protocol_link_obj["fund_bis"]
                        .as_u64()
                        .unwrap_or(site_start + 1)
                        + 1;
                    // let Some(site_end) = protocol_link_obj["fund_bis"].as_u64() else {
                    //     continue;
                    // };

                    match extractor
                        .text_accumulation(
                            &url[..last_idx + 1],
                            site_start as usize..=site_end as usize,
                            |_, _| false,
                            true,
                            "SEITE_",
                        )
                        .await
                    {
                        Some(html_text) => html_text,
                        None => continue,
                    }
                } else {
                    match extractor
                        .extract_text_from_url(&url, Some(find_optimal_votes_by_name), true)
                        .await
                    {
                        Some(html_text) => html_text,
                        None => continue,
                    }
                };

                // println!("variant: {html_text}");
                if let Some(new_delegate_votes) = extract_votes_by_name(&html_text) {
                    delegate_votes = Some(new_delegate_votes);
                    break;
                } else {
                    continue;
                }
            }
            hidden_votes.voted_by_name = true;
            hidden_votes.voted_by_name_data = Some(VotedByName {
                delegates_pro_contra: delegate_votes,
                vote_sum,
                pro_count,
                contra_count,
                invalid_count,
            });
            // println!("hidden_votes: {hidden_votes:?}");
            break;
        }

        hidden_votes.pro_contra_parties = extract_pro_contra_party_codes_from_string(text);
        if hidden_votes.pro_contra_parties.is_some() {
            break;
        } else {
            let text = text.to_lowercase();
            if text.contains("einhellig") || text.contains("einstimmig") {
                hidden_votes.voted_unanimously = true
            }
        }
    }

    (plenary_session, Some(hidden_votes))
}

pub fn extract_vote_count(text: &str) -> Option<(usize, usize, usize, usize)> {
    let vote_counts_regex = yes_no_votes_regex();
    let prefixed_vote_counts_regex = yes_no_votes_prefixed_regex();

    let captures = match vote_counts_regex.captures(text) {
        Some(captures) => captures,
        None => prefixed_vote_counts_regex.captures(text)?,
    };
    Some((
        captures.get(1)?.as_str().parse().unwrap(),
        captures.get(2)?.as_str().parse().unwrap(),
        captures.get(3)?.as_str().parse().unwrap(),
        captures
            .get(4)
            .map(|cap| cap.as_str().parse::<usize>().unwrap_or_default())
            .unwrap_or(0),
    ))
}

pub fn extract_votes_by_name(from_html_text: &str) -> Option<ProContra> {
    let yes_no_regex = yes_no_votes_inside_regex();
    let start_of_yes_no_idx = yes_no_regex
        .find(from_html_text)
        .map(|x| x.end())
        .unwrap_or_default();
    let from_html_text = &from_html_text[start_of_yes_no_idx..];
    let from_html_text = &from_html_text.replace("Graf Herbert L.", "Graf Herbert L");

    // println!("from html text: {from_html_text}");

    let (pro_regex, contra_regex) = crate::pro_contra_named_vote_indicators();

    let pro_match = pro_regex.find(from_html_text)?;
    let contra_match: regex::Match<'_> = contra_regex.find(from_html_text)?;

    // dbg!(from_html_text[pro_match.start()..pro_match.end()]
    //     .chars()
    //     .collect::<String>());
    // dbg!(from_html_text[contra_match.start()..contra_match.end()]
    //     .chars()
    //     .collect::<String>());

    let pro_open_ended = &from_html_text[pro_match.end()..];
    let end_pro_idx = pro_open_ended.find('.');
    let pro_delegates = match end_pro_idx {
        Some(end_pro_idx) => {
            let contra_idx = contra_match.start() - pro_match.end();
            let end_pro_idx = contra_idx.min(end_pro_idx);
            &pro_open_ended[..end_pro_idx]
        }
        None => &from_html_text[pro_match.end()..contra_match.start() - 1],
    };

    // cannot use start end end as they are byte offsets: this leads to wrong bounds for "„Ja“" (not only a single byte long)

    let pro_delegates = group_names_from_vote_section(pro_delegates);

    let contra_open_ended = &from_html_text[contra_match.end() + 1..];
    let end_contra_idx = contra_open_ended.find('.');
    let contra_delegates = match end_contra_idx {
        Some(end_contra_idx) => {
            let star_idx = contra_open_ended.find('*').unwrap_or(end_contra_idx);
            let end_contra_idx = end_contra_idx.min(star_idx);

            &contra_open_ended[..end_contra_idx]
        }
        None => {
            let end_contra_idx = contra_open_ended.find('*')?;

            let x = &contra_open_ended[..end_contra_idx - 1];
            dbg!(x);
            x
        }
    };

    let contra_delegates = group_names_from_vote_section(contra_delegates);

    Some(ProContra {
        pro: pro_delegates,
        contra: contra_delegates,
    })
}
