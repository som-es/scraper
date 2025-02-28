use common_scrapes::{HiddenVotes, ProContra, Stage, VoteStage, VoteType, VotedByName};
use regex::Regex;
use serde_json::Value;

use crate::{
    extract_pro_contra_party_codes_from_string, find_optimal_votes_by_name,
    legis_init::group_names_from_vote_section, yes_no_votes_inside_regex, yes_no_votes_regex,
    AustriaScraper, Scraper, TextExtractor,
};

use super::extract_invisibly_declined_from_text;

pub async fn extract_hidden_votes(legis_content: &Value) -> (Option<u64>, Option<HiddenVotes>) {
    let mut most_fitting_votes = (None, None);
    let status = legis_content.get("status");

    if legis_content.get("phase").is_none() {
        if let Some(stages) = legis_content.get("stages") {
            most_fitting_votes = extract_most_fitting_votes(stages, status).await;
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

        if phase["name"]
            .as_str()
            .unwrap()
            .contains("Plenarberatungen NR")
        {
            most_fitting_votes = extract_most_fitting_votes(stages, status).await;
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
    } else if text.contains("antrag") {
        VoteType::Proposal
    // https://www.parlament.gv.at/gegenstand/XXVII/III/835
    } else if text.contains("Antrag auf Kenntnisnahme") {
        VoteType::Proposal
    } else if text.to_lowercase().contains("gesetzesvorschlag") {
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

pub async fn extract_most_fitting_votes(
    stages: &Value,
    status: Option<&Value>,
) -> (Option<u64>, Option<HiddenVotes>) {
    let sitzung_id_regex = Regex::new(r"(\d+). Sitzung").unwrap();
    let is_site_stored_regex = Regex::new(r"SEITE_\d+.html").unwrap();

    let mut hidden_votes = HiddenVotes::default();
    hidden_votes.vote_stage = status
        .map(|status| status["description"].as_str())
        .flatten()
        .map(|desc| {
            let desc = &desc[desc.find("Nationalrat").unwrap_or_default()..];
            classify_vote_stage(desc)
        })
        .flatten();

    let mut plenary_session = None;

    for stage in stages.as_array().unwrap().iter().rev() {
        let fsth = stage["fsth"].as_array().cloned().unwrap_or_default();

        let Some(text) = stage["text"].as_str() else {
            continue;
        };

        if !(text.contains("Abstimmung")
            || text.contains("abgelehnt")
            || text.contains("angenommen"))
            || (text.to_lowercase().contains("ausschuss") && !text.contains("Nationalrat"))
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

                // let start_site = protocol_link_obj["fund_von"].as_u64();
                // let end_site = protocol_link_obj["fund_bis"].as_u64();
                // match (start_site, end_site) {
                //     (Some(start_site), Some(end_site)) => {
                //         // ahhh
                //     }
                //     (None, None) => {
                //         // fine
                //     }
                //     _ => panic!("should not be possible hopefully"),
                // }
                let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
                // dbg!(protocol_link_obj);

                let html_text = if is_site_stored_regex.is_match(url) {
                    let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '/').unwrap();

                    let Some(site_start) = protocol_link_obj["fund_von"].as_u64() else {
                        continue;
                    };

                    let site_end = protocol_link_obj["fund_bis"]
                        .as_u64()
                        .unwrap_or(site_start + 1);
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
        }
    }

    (plenary_session, Some(hidden_votes))
}

pub fn extract_vote_count(text: &str) -> Option<(usize, usize, usize, usize)> {
    let vote_counts_regex = yes_no_votes_regex();

    let captures = vote_counts_regex.captures(text)?;
    Some((
        captures.get(1).unwrap().as_str().parse().unwrap(),
        captures.get(2).unwrap().as_str().parse().unwrap(),
        captures.get(3).unwrap().as_str().parse().unwrap(),
        captures
            .get(4)
            .map(|cap| cap.as_str().parse::<usize>().unwrap_or_default())
            .unwrap_or(0),
    ))
}

fn extract_votes_by_name(from_html_text: &str) -> Option<ProContra> {
    let yes_no_regex = yes_no_votes_inside_regex();
    let start_of_yes_no_idx = yes_no_regex
        .find(from_html_text)
        .map(|x| x.end())
        .unwrap_or_default();
    let from_html_text = &from_html_text[start_of_yes_no_idx..];
    let from_html_text = &from_html_text.replace("Graf Herbert L.", "Graf Herbert L");

    let pro_regex =
        Regex::new(r"(?m)Mit\s*.*Ja.*\s*stimmten\s*die\s*Abgeordneten\s*[()\sa-zA-Z]*\s*:")
            .unwrap();
    let contra_regex =
        Regex::new(r"(?m)Mit\s*.*Nein.*\s*stimmten\s*die\s*Abgeordneten\s*[()\sa-zA-Z]*\s*:")
            .unwrap();

    // println!("form {from_html_text}");
    let pro_match = pro_regex.find(from_html_text)?;
    let contra_match: regex::Match<'_> = contra_regex.find(from_html_text)?;
    // println!("from html text: {from_html_text}");

    // dbg!(from_html_text[pro_match.start()..pro_match.end()]
    //     .chars()
    //     .collect::<String>());
    // dbg!(from_html_text[contra_match.start()..contra_match.end()]
    //     .chars()
    //     .collect::<String>());

    // let from_html_text = from_html_text.chars().collect::<Vec<char>>();
    // let pro_open_ended = &from_html_text[pro_votes_start_idx..];
    // let pro_open_ended = &pro_open_ended[pro_open_ended.iter().position(|x| *x == ':')? + 1..];
    let pro_open_ended = &from_html_text[pro_match.end()..];
    // let end_pro_idx = pro_open_ended.iter().position(|x| *x == '.')?;
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

    // let pro_delegates = group_names_from_vote_section(&pro_delegates.iter().collect::<String>());
    let pro_delegates = group_names_from_vote_section(pro_delegates);
    // println!("------------ pro_delegates: {pro_delegates:?}");
    // println!("--------------- text: {from_html_text}");

    // dbg!(contra_votes_start_idx);
    // let contra_open_ended = &from_html_text[contra_votes_start_idx..];
    // let contra_open_ended = &contra_open_ended[contra_open_ended.iter().position(|x| *x == ':')? + 1..];
    let contra_open_ended = &from_html_text[contra_match.end() + 1..];
    // let end_contra_idx = contra_open_ended.iter().position(|x| *x == '.')?;
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

    // let contra_delegates = group_names_from_vote_section(&contra_delegates.iter().collect::<String>());
    let contra_delegates = group_names_from_vote_section(contra_delegates);

    // println!("-------------------- contra_delegates: {contra_delegates:?}");
    Some(ProContra {
        pro: pro_delegates,
        contra: contra_delegates,
    })
}
