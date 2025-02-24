use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::{
    AcceptedState, HiddenVotes, LegislativeInitiative, Party, Reference, Speech, Vote, VoteType,
    VotedByName,
};
use regex::Regex;
use serde_json::{json, Value};

use crate::{
    caching::{disk_caching, get_disk_cache_json, get_disk_cache_json_with_secs},
    documents,
    error::ScraperError,
    extract_from_gegenstand_url, extract_from_gegenstand_url_text,
    extract_pro_contra_party_codes_from_string, extract_text_from_html,
    filter::filter_name_extraction,
    find_optimal_votes_by_name, person_url_to_id,
    proposals::ProposalScraper,
    speeches::extract_speeches,
    yes_no_votes_inside_regex, yes_no_votes_regex, AustriaScraper, DeclineUrlGegenstand,
    GegenstandPaths, ProContra, Scraper, TextExtractor,
};

// const PARTIES: LazyLock<Vec<Party>> = LazyLock::new(|| {
//     let scraper = AustriaScraper::default();
//     pollster::block_on(scraper.get_parties()).unwrap()
// });

#[async_trait]
pub trait LegislativeInitiatives: Scraper {
    const FILTER_PATH: &'static str;

    fn legis_payload(&self) -> Self::Payload;

    async fn post_legis_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.legis_payload();
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

    async fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        for ityp in IGNORE_ITYPS {
            if proposal_path.contains(&format!("{ityp}/")) {
                return Err(ScraperError::NoVoteFound);
            }
        }
        let raw = match get_disk_cache_json(proposal_path).await {
            Some(raw) => raw,
            None => {
                log::info!("NO CACHE!!!!!!!! {proposal_path:?}");
                let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
                if let Err(e) = disk_caching(proposal_path, raw_proposal.to_string()).await {
                    log::warn!("could cache raw {proposal_path}: {e:?}")
                }
                raw_proposal
            }
        };
        Self::raw_legis_into_legis(raw)
            .await
            .ok_or(ScraperError::LegisInitParseError)
    }
}

pub const IGNORE_ITYPS: [&str; 3] = ["J", "EUBTG", "AB"];

#[async_trait]
impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/filter/data/101?js=eval&showAll=true&export=true";
    // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=undefined&showAll=true&export=true";
    // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=undefined&showAll=true&export=true";

    fn legis_payload(&self) -> Self::Payload {
        json!({
            "GP": self.scrape_info.gp,
            "NRBR": self.scrape_info.council_type,
            /*"DOKTYP": [
                "VOLKBG",
                "RV",
                "RVS",
                "A(E)",
                "A",
                "AA",
                "AEA",
                "AMIN",
                "UEAM",
                "UEA",
                "BUA",
                "BRA",
                "ARH2",
                "AVB",
                "URH2",
                "ME",
                "AUB"
            ],*/
            // "VHG": [
            //     "ANTR"
            // ],
            "DATUM_VON": [
                self.scrape_info.date_start, self.scrape_info.date_end
            ],
        })

        // json!({
        //     "GP": self.scrape_info.gp,
        //     "RGES": [
        //         "ALLE"
        //     ]
        // })
    }

    async fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        // let json = serde_json::to_string_pretty(&raw_legis)?;
        // println!("{json}");

        let legis_content = &raw_legis["content"];

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let doktyp = legis_content["doktyp"].as_str()?.to_string();

        if IGNORE_ITYPS.contains(&ityp.as_str()) {
            return None;
        }

        let inr = legis_content["inr"].as_u64()?;
        let gp = legis_content["gp_code"].as_str()?.to_string();

        let raw_documents = if let Some(preconst) = legis_content.get("preconst") {
            let preconst = &preconst.as_array()?[0];
            preconst["documents"].as_array()?
        } else {
            legis_content["documents"].as_array()?
        };
        let documents = documents(raw_documents)?;

        let names = legis_content["names"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let delegate_ids = names
            .iter()
            .flat_map(|name| name["url"].as_str().map(person_url_to_id))
            .flatten()
            .collect::<Vec<u64>>();

        // let votes = extract_votes(legis_content)?;
        let votes = extract_votes(legis_content).unwrap_or_default();
        let topics = extract_topics(legis_content, "topics").unwrap_or_default();
        let mut eurovoc_topics = extract_topics(legis_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics = extract_topics(legis_content, "headwords").unwrap_or_default();

        let mut speeches = extract_speeches(legis_content, true).unwrap_or_default();
        let date = extract_date(legis_content)?;

        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y ").ok()?;

        let mut accepted = if votes.is_empty() {
            None
        } else {
            legis_content["vote"]["infavor"].as_bool()
        }
        .map(|x| {
            if x {
                AcceptedState::Accepted
            } else {
                AcceptedState::Declined
            }
        });

        let emphasis: Option<String> = if let Some(shortinfo) = legis_content.get("shortinfo") {
            let teil1 = extract_text_from_html(shortinfo["teil1"].as_str().unwrap_or_default());
            let teil2 = extract_text_from_html(shortinfo["teil2"].as_str().unwrap_or_default());
            if teil1.is_empty() && teil2.is_empty() {
                None
            } else {
                Some(format!("{teil1} {teil2}"))
            }
        } else {
            None
        };

        let created_at = legis_content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;

        let updated_at = legis_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let requires_simple_majority =
            extract_majority_on_indicators(legis_content).unwrap_or(true);
        let (was_invisibly_declined, _) = extract_invisibly_declined(legis_content)?;

        let (plenary_session, hidden_votes) = extract_hidden_votes(legis_content).await;
        let mut hidden_votes = hidden_votes.unwrap_or_default();

        let references = extract_references(legis_content);

        // sometimes, the info is missing
        if hidden_votes.voted_by_name && !eurovoc_topics.contains(&"namentliche Abstimmung".into())
        {
            eurovoc_topics.push("namentliche Abstimmung".into());
        }
        if hidden_votes.voted_by_name && hidden_votes.voted_by_name_data.is_some() {
            let voted_by_name_data = hidden_votes.voted_by_name_data.as_ref().unwrap();
            accepted = Some(is_accepted_based_on_votes(
                voted_by_name_data.vote_sum,
                voted_by_name_data.pro_count,
                requires_simple_majority,
            ))
        }
        if accepted.is_none() && was_invisibly_declined {
            accepted = Some(AcceptedState::PreDeclined);

            // proposals are inversed declined warum auch immer !
            if let (Some(VoteType::ReversingProposal), Some(pro_contra)) =
                (hidden_votes.vote_type, &mut hidden_votes.pro_contra_parties)
            {
                core::mem::swap(&mut pro_contra.contra, &mut pro_contra.pro);
                speeches = speeches
                    .into_iter()
                    .map(|speech| Speech {
                        delegate_id: speech.delegate_id,
                        delegate_display_name: speech.delegate_display_name,
                        opinion: speech.opinion.invert(),
                        spoken_text_html_urls: speech.spoken_text_html_urls,
                    })
                    .collect();
            }
        };

        // /gegenstand/XXII/A/231
        // if accepted.is_some() {
        //     sitzung_id.unwrap();
        // }

        // if !voted_by_name && eurovoc_topics.contains(&"namentliche Abstimmung".to_string()) {
        //     println!("--> this is interesting")
        // }

        // this also shows stuff for the bundesrat:
        // this may not be enough, go through every stage in
        // let voted_by_name = eurovoc_topics.contains(&"namentliche Abstimmung".to_string());

        let id = format!("{gp}/{ityp}/{inr}");

        let proposal = <Self as ProposalScraper>::raw_proposal_into_proposal(raw_legis)?;

        Some(LegislativeInitiative {
            title,
            description,
            emphasis,
            inr,
            doktyp,
            // council: todo!(),
            gp,
            ityp,
            id,
            votes,
            documents,
            delegate_ids,
            accepted,
            date,
            speeches,
            topics,
            eurovoc_topics,
            other_keyword_topics,
            created_at,
            updated_at,
            requires_simple_majority,
            proposal,
            hidden_votes_info: hidden_votes,
            plenary_session,
            references,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12 + 2)
            .ok_or(ScraperError::LegisInitParseError)
    }
}

pub fn extract_references(legis_content: &Value) -> Option<Vec<Reference>> {
    Some(
        legis_content["reference"]
            .as_array()?
            .iter()
            .flat_map(|v| {
                let (gp, ityp, inr) = extract_from_gegenstand_url(&v["url"].as_str()?[2..])?;
                Some(Reference {
                    gp,
                    ityp,
                    inr: inr?,
                })
            })
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

pub fn extract_invisibly_declined(
    legis_content: &Value,
) -> Option<(bool, Option<DeclineUrlGegenstand>)> {
    let status_description = legis_content["status"]["description"].as_str();

    let Some(status_description) = status_description else {
        return Some((false, None));
    };

    if status_description.contains("abgelehnt") {
        return Some((true, extract_from_gegenstand_url_text(status_description)));
    }

    Some((false, None))
}

pub fn extract_date(legis_content: &Value) -> Option<&str> {
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

        if !phase["name"].as_str()?.contains("NR") {
            continue;
        }

        // extract_speeches_from_stages(stages, &mut speeches);
        most_fitting_date = extract_most_fitting_date(stages);
    }
    most_fitting_date
}

pub async fn extract_hidden_votes(legis_content: &Value) -> (Option<u64>, Option<HiddenVotes>) {
    let mut most_fitting_votes = (None, None);

    if legis_content.get("phase").is_none() {
        if let Some(stages) = legis_content.get("stages") {
            most_fitting_votes = extract_most_fitting_votes(stages).await;
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
            most_fitting_votes = extract_most_fitting_votes(stages).await;
            break;
        }
    }
    most_fitting_votes
}

pub async fn extract_most_fitting_votes(stages: &Value) -> (Option<u64>, Option<HiddenVotes>) {
    let sitzung_id_regex = Regex::new(r"(\d+). Sitzung").unwrap();
    let is_site_stored_regex = Regex::new(r"SEITE_\d+.html").unwrap();

    let mut hidden_votes = HiddenVotes::default();
    let mut plenary_session = None;

    for stage in stages.as_array().unwrap().iter().rev() {
        let fsth = stage["fsth"].as_array().cloned().unwrap_or_default();

        let Some(text) = stage["text"].as_str() else {
            continue;
        };

        if !(text.contains("Abstimmung")
            || text.contains("abgelehnt")
            || text.contains("angenommen"))
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

        if text.contains("Gesetzesvorschlag in zweiter Lesung") {
            hidden_votes.vote_type = Some(VoteType::Second)
        } else if text.contains("Gesetzesvorschlag in dritter Lesung") {
            hidden_votes.vote_type = Some(VoteType::Third)
        } else if text.contains("erster Lesung") {
            hidden_votes.vote_type = Some(VoteType::First)
        } else if text.contains("Antrag, den ablehnenden Ausschussbericht") {
            hidden_votes.vote_type = Some(VoteType::ReversingProposal)
        } else if text.contains("antrag") {
            hidden_votes.vote_type = Some(VoteType::Proposal)
        // https://www.parlament.gv.at/gegenstand/XXVII/III/835
        } else if text.contains("Antrag auf Kenntnisnahme") {
            hidden_votes.vote_type = Some(VoteType::Proposal)
        } else if text.contains("Sitzung des Nationalrates")
            && text.contains("Genehmigung")
            && text.to_lowercase().contains("namentliche")
        {
            hidden_votes.vote_type = Some(VoteType::OnlyVotedByName)
        } else if text.to_lowercase().contains("namentliche abstimmung") {
            hidden_votes.vote_type = Some(VoteType::AnyNamedVote)
        } else {
            // need to find fitting type, some stages are bs
            continue;
        }

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
                dbg!(protocol_link_obj);

                let html_text = if is_site_stored_regex.is_match(url) {
                    let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '/').unwrap();

                    let Some(site_start) = protocol_link_obj["fund_von"].as_u64() else {
                        continue;
                    };
                    let Some(site_end) = protocol_link_obj["fund_bis"].as_u64() else {
                        continue;
                    };

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

fn extract_vote_count(text: &str) -> Option<(usize, usize, usize, usize)> {
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

    let pro_regex = Regex::new(r"(?m)Mit\s*.*Ja.*\s*stimmten\s*die\s*Abgeordneten\s*[()\sa-zA-Z]*\s*:").unwrap();
    let contra_regex =
        Regex::new(r"(?m)Mit\s*.*Nein.*\s*stimmten\s*die\s*Abgeordneten\s*[()\sa-zA-Z]*\s*:").unwrap();

    // println!("form {from_html_text}");
    let pro_match = pro_regex.find(from_html_text)?;
    let contra_match: regex::Match<'_> = contra_regex.find(from_html_text)?;


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
    let end_pro_idx = pro_open_ended.find('.')?;
    let pro_delegates = &pro_open_ended[..end_pro_idx];

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
    let end_contra_idx = contra_open_ended.find('.')?;
    let contra_delegates = &contra_open_ended[..end_contra_idx];

    // let contra_delegates = group_names_from_vote_section(&contra_delegates.iter().collect::<String>());
    let contra_delegates = group_names_from_vote_section(contra_delegates);

    // println!("-------------------- contra_delegates: {contra_delegates:?}");
    Some(ProContra {
        pro: pro_delegates,
        contra: contra_delegates,
    })
}

fn group_names_from_vote_section(names: &str) -> Vec<String> {
    filter_name_extraction(names.split([',', ';'])).collect()
}

fn extract_majority_on_indicators(legis_content: &Value) -> Option<bool> {
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

fn extract_most_fitting_date(raw_stages: &Value) -> Option<&str> {
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
    Some(
        legis_content[topics_name]["data"]["bubbles"]
            .as_array()?
            .iter()
            .flat_map(|topic| topic["label"].as_str().map(|topic| topic.to_string()))
            .collect(),
    )
}

fn extract_votes(legis_content: &Value) -> Option<Vec<Vote>> {
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

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common_scrapes::{AcceptedState, Opinion, Party, VoteType};

    use crate::{
        extract_text_from_html,
        legis_init::{LegislativeInitiatives, Vote},
        AustriaScraper, Scraper,
    };

    use super::is_accepted_based_on_votes;

    #[test]
    fn test_accepted_on_vote_count_stuff() {
        let accepted = is_accepted_based_on_votes(100, 50, true);
        assert_eq!(accepted, AcceptedState::Accepted);

        let accepted = is_accepted_based_on_votes(120, 50, true);
        assert_eq!(accepted, AcceptedState::Declined);

        let accepted = is_accepted_based_on_votes(120, 65, false);
        assert_eq!(accepted, AcceptedState::Declined);

        let accepted = is_accepted_based_on_votes(120, 80, false);
        assert_eq!(accepted, AcceptedState::Accepted);

        let accepted = is_accepted_based_on_votes(120, 79, false);
        assert_eq!(accepted, AcceptedState::Declined);
    }

    #[test]
    fn test_text_extract() {
        let html = r#"<h4>Schwerpunkte der Regierungsvorlage</h4>\n\n<ul class=\"unordered-list\">\n\t<li>&Auml;nderung des &sect; 46 KFG und Schaffung exakterer Vorgaben f&uuml;r die Bewilligung von &Uuml;berstellungsfahrten</li>\n\t<li>Entsprechende &Auml;nderung des &sect; 40 und des &sect; 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister &uuml;bernehmen k&ouml;nnen. Zus&auml;tzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgef&uuml;hrt werden</li>\n</ul>\n"#;

        let text = extract_text_from_html(html);
        assert_eq!("Schwerpunkte der Regierungsvorlage\\n\\n\\n\\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\\n\\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\\n\\n", text);
    }

    #[tokio::test]
    async fn test_legis_init_all() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXVII".to_string()];

        let initiatives = scraper.post_legis_payload().await.unwrap();
        let paths = scraper.legis_paths(initiatives).unwrap();
        assert!(paths.len() >= 10);
        println!("length: {}", paths.len());

        // return;

        let mut error_count = 0;
        for path in &paths {
            // let x = scraper.get_legis_init(path).await;
            // if x.is_err() {
            //     error_count += 1;
            //     println!("erorring path: {path}");
            // }
            if path.contains("I/1858") {
                println!("path: {path:?}");
            }
            // println!("path: {path:?}");
        }
        println!("paths: {}", paths.len());
        println!("error count: {error_count}");
    }

    // /gegenstand/XXVII/A/4129

    #[tokio::test]
    async fn test_get_legis_init_invisibly_declined_2_3_majority() {
        let scraper = AustriaScraper::default();
        let legis_init = scraper
            .get_legis_init("/gegenstand/XXVII/A/4129")
            .await
            .unwrap();
        assert_eq!(legis_init.delegate_ids, vec![5439, 87001]);
        assert!(!legis_init.requires_simple_majority);
        assert_eq!(legis_init.accepted.unwrap(), AcceptedState::PreDeclined);
        assert!(!legis_init.speeches.is_empty());
        let party_codes = legis_init.hidden_votes_info.pro_contra_parties.unwrap();
        assert_eq!(party_codes.pro, vec!["V".to_string(), "G".to_string()]);
        assert_eq!(
            party_codes.contra,
            vec!["S".to_string(), "F".to_string(), "N".to_string()]
        );
        assert_eq!(
            legis_init.hidden_votes_info.vote_type.unwrap(),
            VoteType::Second
        );
    }

    #[tokio::test]
    async fn test_get_legis_init_was_voted_by_name() {
        let mut scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let mut count = 0;
        // for gp in ["XXIV", "XXIII", "XXII"] {
        for gp in [
            "XXVIII", "XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX",
        ] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let initiatives = scraper.post_legis_payload().await.unwrap();
            for path in scraper.legis_paths(initiatives).unwrap() {
                // println!();
                // println!("path: {path:?}");
                let Ok(init) = scraper.get_legis_init(&path).await else {
                    println!("error {}{path}", AustriaScraper::BASE_URL);
                    continue;
                };
                // println!("init: {init:?}");
                if init.hidden_votes_info.voted_by_name {
                    println!("init: title: {}, accepted: {:?}, votes: {:?}, id: {:?}, hidden_votes: {:?}, ", init.title, init.accepted, init.votes, init.id, init.hidden_votes_info);
                    println!(" {}{path}", AustriaScraper::BASE_URL,);
                    if let Some(c) = init.hidden_votes_info.voted_by_name_data {
                        if let Some(d) = c.delegates_pro_contra {
                            count += 1;
                        }
                    }
                }
            }
            println!("elapsed: {:?}", start.elapsed());
            println!("count: {count:?}");
            // break;
        }

        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_was_invisibly_declined() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        let mut count = 0;
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("error {}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            if init.accepted == Some(AcceptedState::PreDeclined) {
                println!(
                    "{}, {hidden:?}, {ty:?} was_invisibly_declined: {:?} {}{path}",
                    init.hidden_votes_info.voted_by_name,
                    init.accepted,
                    AustriaScraper::BASE_URL,
                    hidden = init.hidden_votes_info.pro_contra_parties,
                    ty = init.hidden_votes_info.vote_type
                );
                count += 1;
            }
        }

        println!("elapsed: {:?}", start.elapsed());
        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_majority() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            //println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("error {}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            // if init.accepted == Some(false) {
            // println!("{:?} 1/2 majority?: {} {}{path}", init.accepted,init.requires_simple_majority, AustriaScraper::BASE_URL);
            if !init.requires_simple_majority {
                println!("---------------------------------");
            }
            println!(
                "1/2 majority?: {} {}{path}",
                init.requires_simple_majority,
                AustriaScraper::BASE_URL
            );
            if !init.requires_simple_majority {
                println!("---------------------------------");
            }
            // }
            // println!("{:?} {}{path}", init.accepted, AustriaScraper::BASE_URL);
            // println!("topics: {:?}", init.topics);
            // println!("id: {}, title: {}", init.id, init.title);
        }

        println!("elapsed: {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn test_get_legis_init_xxvii_a_3373_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3373")
            .await
            .unwrap();
        println!("init: {init:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_xxvii_i_1774_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1774")
            .await
            .unwrap();
        println!("init: {init:?}");

        assert_eq!(init.date, NaiveDate::from_ymd_opt(2023, 6, 1).unwrap());

        assert_eq!(
            init.description,
            "Strompreiskosten-Ausgleichsgesetz 2022, SAG 2022 (1774 d.B.)"
        );
    }
    #[tokio::test]
    async fn test_get_legis_init_xxi_a_412() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXI/A/412")
            .await
            .unwrap();

        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));
    }

    // https://www.parlament.gv.at/gegenstand/XXII/I/451
    #[tokio::test]
    async fn test_get_legis_init_xxii_i_451() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXII/I/451")
            .await
            .unwrap();

        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));

        #[rustfmt::skip]
        let lhs = vec![
                "Achleitner", "Amon", "Auer Jakob", "Auer Klaus Hubert", "Baumgartner-Gabitzer", "Bleckmann", "Böhm", "Bösch", "Brader Alfred", "Brinek", "Bucher", "Dolinschek", "Donabauer Karl", "Donnerbauer Heribert", "Doppler", "Ellmauer", "Eßl", "Fasslabend", "Fekter", "Felzmann", "Franz", "Freund", "Fuhrmann", "Gahr Hermann", "Glaser", "Grander", "Grillitsch", "Großruck", "Hakl", "Haubner", "Hofmann", "Höllerer", "Hornek", "Huainigg", "Hütl", "Ikrath", "Kainz", "Kapeller", "Keuschnigg", "Khol", "Kopf", "Kößl", "Kurzbauer", "Langreiter", "Ledolter", "Lentsch", "Liechtenstein Vincenz", "Lichtenegger Elmar", "Lopatka", "Machne", "Maier Ferdinand", "Mainoni", "Marek", "Mikesch", "Missethon", "Mitterlehner", "Molterer", "Murauer", "Neudeck", "Neugebauer", "Pack", "Partik-Pablé", "Praßl", "Preineder", "Prinz", "Prinzhorn", "Rädler Johann", "Rasinger", "Regler Roderich", "Riener", "Rosenkranz", "Rossmann", "Scheibner", "Scheuch", "Scheucher-Pichler", "Schiefermair", "Schöls", "Schultes", "Schweisgut", "Sieber", "Sonnberger", "Spindelegger Michael", "Stadler", "Steibl Ridi", "Steindl Konrad", "Stummvoll", "Tamandl", "Tancsits", "Turković-Wendl", "Walch", "Wattaul", "Winkler", "Wittauer", "Wöginger", "Wolfmayr", "Zweytick"
            ];
        for (buf, r) in init
            .hidden_votes_info
            .clone()
            .voted_by_name_data
            .unwrap()
            .delegates_pro_contra
            .unwrap()
            .pro
            .iter()
            .zip(&lhs)
        {
            assert_eq!(r, buf);
        }

        #[rustfmt::skip]
        let rhs = vec![
            "Bauer", "Bayr", "Becher", "Binder", "Brosz", "Broukal", "Bures", "Cap", "Csörgits", "Eder", "Einem", "Faul", "Fleckl", "Gaál Anton", "Gartlehner", "Gaßner", "Glawischnig", "Gradwohl", "Grossmann", "Grünewald", "Gusenbauer", "Haidlmayr", "Heinisch-Hosek", "Heinzl", "Hoscher", "Jarolim", "Kaipel", "Keck", "Kogler", "Königsberger-Ludwig", "Krainer", "Kräuter", "Krist", "Kummerer", "Kuntzl", "Lackner", "Lapp", "Lichtenberger Evelin", "Lunacek", "Maier Johann", "Mandak", "Marizzi", "Matznetter", "Moser Hans", "Muttonen", "Niederwieser", "Nürnberger", "Öllinger", "Parnigoni", "Pendl", "Pfeffer", "Pirklhuber", "Posch", "Prähauser", "Prammer", "Puswald", "Rada Robert", "Reheis", "Rest-Hinterseer", "Riepl", "Sburny", "Scharer", "Schasching", "Schieder", "Schönpass", "Schopf",
            "Silhavy", "Sima", "Spindelberger Erwin", "Stadlbauer", "Steier", "Stoisits", "Trunk", "Van der Bellen", "Walther", "Weinzinger", "Wimmer", "Wittmann", "Wurm"
        ];

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra,
            rhs
        );
    }

    // https://www.parlament.gv.at/gegenstand/XX/I/1428
    #[tokio::test]
    async fn test_get_legis_init_xx_i_1428() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XX/I/1428")
            .await
            .unwrap();

        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .pro,
            vec![
                "Ablinger",
                "Achs",
                "Amon",
                "Antoni",
                "Auer",
                "Bauer Rosemarie",
                "Bauer Sophie",
                "Binder",
                "Brix",
                "Buder",
                "Bures",
                "Donabauer",
                "Eder",
                "Edler",
                "Ellmauer",
                "Feurstein",
                "Fink",
                "Fischer",
                "Freund",
                "Fuchs",
                "Gaál",
                "Gartlehner",
                "Gaßner",
                "Gatterer",
                "Grabner",
                "Gradwohl",
                "Großruck",
                "Gusenbauer",
                "Hagenhofer",
                "Heindl",
                "Heinzl",
                "Hlavac",
                "Höchtl",
                "Horngacher",
                "Kaipel",
                "Kampichler",
                "Kaufmann",
                "Keppelmüller",
                "Khol",
                "Kiermaier",
                "Kiss",
                "König",
                "Konrad",
                "Koppler",
                "Kostelka",
                "Krammer",
                "Kräuter",
                "Kröll",
                "Kukacka",
                "Kummerer",
                "Kurzbauer",
                "Leikam",
                "Leiner",
                "Löschnak",
                "Lukesch",
                "Maier",
                "Maitz",
                "Marizzi",
                "Mertel",
                "Mock",
                "Morak",
                "Moser-Starrach",
                "Mühlbachler",
                "Müller",
                "Murauer",
                "Neisser",
                "Niederwieser",
                "Nowotny",
                "Nürnberger",
                "Oberhaidinger",
                "Parfuss",
                "Parnigoni",
                "Pendl",
                "Pittermann",
                "Platter",
                "Rasinger",
                "Rauch-Kallat",
                "Reitsamer",
                "Riepl",
                "Sauer",
                "Schieder",
                "Schrefel",
                "Schuster",
                "Schwarzböck",
                "Schwarzenberger",
                "Schwemlein",
                "Seidinger",
                "Sigl",
                "Silhavy",
                "Spindelegger",
                "Stampler",
                "Steibl",
                "Steindl",
                "Stippel",
                "Tegischer",
                "Tichy-Schreder",
                "Tychtl",
                "Verzetnitsch",
                "Wimmer",
                "Wurmitzer"
            ]
        );

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra,
            vec![
                "Apfelbeck",
                "Aumayr",
                "Barmüller",
                "Blünegger",
                "Böhacker",
                "Brauneder",
                "Dolinschek",
                "Fekter",
                "Firlinger",
                "Fischl",
                "Frieser",
                "Gaugg",
                "Graf",
                "Gredler",
                "Grollitsch",
                "Haider",
                "Haller",
                "Haupt",
                "Hofmann",
                "Jung",
                "Kammerlander",
                "Kier",
                "Klein",
                "Koller",
                "Kopf",
                "Krüger",
                "Kurzmann",
                "Lafer",
                "Maderthaner",
                "Madl",
                "Marolt",
                "Meischberger",
                "Meisinger",
                "Mentil",
                "Moser Gabriela",
                "Moser Hans Helmut",
                "Nußbaumer",
                "Öllinger",
                "Partik-Pablé",
                "Peter",
                "Petrovic",
                "Povysil",
                "Preisinger",
                "Pumberger",
                "Puttinger",
                "Rieß",
                "Salzl",
                "Schaffenrath",
                "Scheibner",
                "Schöggl",
                "Schweitzer",
                "Stadler",
                "Stummvoll",
                "Trattner",
                "Trinkl",
                "Van der Bellen",
                "Wabl",
                "Wenitsch",
                "Zweytick"
            ]
        );
    }

    // https://www.parlament.gv.at/gegenstand/XXI/I/1183

    #[tokio::test]
    async fn test_get_legis_init_xxi_i_1183() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXI/I/1183")
            .await
            .unwrap();

        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .pro[..39],
            vec![
                "Achatz",
                "Amon",
                "Auer",
                "Bauer Gerhard",
                "Baumgartner-Gabitzer",
                "Blasisker",
                "Böhacker",
                "Brinek",
                "Bruckmann",
                "Brugger",
                "Burket",
                "Dolinschek",
                "Donabauer",
                "Donnerbauer",
                "Egghart",
                "Ellmauer",
                "Fallent",
                "Fasslabend",
                "Fekter",
                "Fink",
                "Firlinger",
                "Freigaßner",
                "Freund",
                "Frieser",
                "Gahr",
                "Gatterer",
                "Graf Herbert L",
                "Graf Martin",
                "Grollitsch",
                "Großruck",
                "Hakl",
                "Haller",
                "Hartinger",
                "Haubner",
                "Hetzl",
                "Hofmann",
                "Hornegger",
                "Hornek",
                "Jung"
            ]
        );

        /*

                Abraham;

        Bauer Hannes, Bauer Sophie, Binder, Brosz, Bures;

        Cap, Csörgits;

        Dietachmayr;

        Eder, Edler, Edlinger;

        Faul, Feurstein, Fischer;

        Gaál, Gartlehner, Gaßner, Glawischnig, Gradwohl, Grünewald, Gusenbauer;

        Hagenhofer, Haidlmayr, Heindl, Heinisch-Hosek, Heinzl, Hlavac, Horn, Huber;

        Jäger, Jarolim;

        Kaipel, Kiermaier, Kopf, Kubitschek, Kummerer, Kuntzl;

        Lackner, Lapp, Lichtenberger, Lunacek;

        Maier, Marizzi, Mertel, Moser, Muttonen;

        Niederwieser, Nürnberger;

        Oberhaidinger, Öllinger;

        Parfuss, Parnigoni, Pendl, Petrovic, Pirklhuber, Posch, Prähauser, Prammer;

        Rada, Reheis, Riepl;

        Schasching, Schieder, Schwemlein, Silhavy, Sima, Stoisits;

        Van der Bellen, Verzetnitsch;

        Wimmer, Wittmann. */

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra,
            vec![
                "Abraham",
                "Bauer Hannes",
                "Bauer Sophie",
                "Binder",
                "Brosz",
                "Bures",
                "Cap",
                "Csörgits",
                "Dietachmayr",
                "Eder",
                "Edler",
                "Edlinger",
                "Faul",
                "Feurstein",
                "Fischer",
                "Gaál",
                "Gartlehner",
                "Gaßner",
                "Glawischnig",
                "Gradwohl",
                "Grünewald",
                "Gusenbauer",
                "Hagenhofer",
                "Haidlmayr",
                "Heindl",
                "Heinisch-Hosek",
                "Heinzl",
                "Hlavac",
                "Horn",
                "Huber",
                "Jäger",
                "Jarolim",
                "Kaipel",
                "Kiermaier",
                "Kopf",
                "Kubitschek",
                "Kummerer",
                "Kuntzl",
                "Lackner",
                "Lapp",
                "Lichtenberger",
                "Lunacek",
                "Maier",
                "Marizzi",
                "Mertel",
                "Moser",
                "Muttonen",
                "Niederwieser",
                "Nürnberger",
                "Oberhaidinger",
                "Öllinger",
                "Parfuss",
                "Parnigoni",
                "Pendl",
                "Petrovic",
                "Pirklhuber",
                "Posch",
                "Prähauser",
                "Prammer",
                "Rada",
                "Reheis",
                "Riepl",
                "Schasching",
                "Schieder",
                "Schwemlein",
                "Silhavy",
                "Sima",
                "Stoisits",
                "Van der Bellen",
                "Verzetnitsch",
                "Wimmer",
                "Wittmann"
            ]
        )
    }

    // https://www.parlament.gv.at/gegenstand/XXVII/A/2068
    #[tokio::test]
    async fn test_get_legis_init_voted_by_name_xxvii_a_2068() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/2068")
            .await
            .unwrap();
        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Declined));
        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .pro[..8],
            vec![
                "Baumgartner",
                "Berlakovich Nikolaus",
                "Bernhard",
                "Blimlinger",
                "Brandstätter Helmut",
                "Brandstötter Henrike",
                "Brandweiner",
                "Bürstmayr"
            ]
        );
        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra[..8],
            vec![
                "Amesbauer",
                "Angerer",
                "Bayr",
                "Becher",
                "Belakowitsch Dagmar",
                "Bösch",
                "Brückl",
                "Bures",
            ]
        )
    }

    // gegenstand/XXII/I/653

    #[tokio::test]
    async fn test_get_legis_init_voted_by_name_xxii_i_653() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXII/I/653")
            .await
            .unwrap();
        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .pro[..8],
            vec![
                "Achleitner",
                "Amon",
                "Auer Jakob",
                "Auer Klaus Hubert",
                "Baumgartner-Gabitzer",
                "Böhm",
                "Böhmdorfer",
                "Bösch"
            ]
        );
        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra[..8],
            vec!["Bauer", "Bayr", "Becher", "Binder", "Brosz", "Broukal", "Bures", "Cap"]
        )
    }

    // gegenstand/XXVI/A/303

    #[tokio::test]
    async fn test_get_legis_init_xxvi_a_303() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVI/A/303")
            .await
            .unwrap();
        assert!(init.hidden_votes_info.voted_by_name);
        assert_eq!(init.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .pro[..8],
            vec![
                "Amesbauer",
                "Amon Werner",
                "Angerer",
                "Baumgartner",
                "Belakowitsch Dagmar",
                "Berger",
                "Berlakovich Nikolaus",
                "Bernhard"
            ]
        );

        assert_eq!(
            init.hidden_votes_info
                .clone()
                .voted_by_name_data
                .unwrap()
                .delegates_pro_contra
                .unwrap()
                .contra[..8],
            vec![
                "Androsch",
                "Antoni",
                "Bacher Walter",
                "Bayr",
                "Becher Ruth",
                "Bißmann",
                "Cox",
                "Duzdar Muna"
            ]
        )
    }

    #[tokio::test]
    async fn test_get_legis_init_i_1954_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        assert_eq!(init.topics, vec!["Verkehr und Infrastruktur".to_string()]);

        assert_eq!(
            init.title,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.) | Parlament Österreich"
        );
        assert_eq!(
            init.description,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.)"
        );

        assert_eq!(init.gp, "XXVII");
        assert_eq!(init.emphasis, Some("Schwerpunkte der Regierungsvorlage\n\n\n\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\n\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\n\n \n\tNeuregelung der Ausbildung und Prüfung des in den Fahrschulen eingesetzten Lehrpersonals, wobei ein stärkerer Praxisbezug hergestellt wird und auch eine verpflichtende Aus- und Weiterbildung vorgeschrieben wird\n\tSchaffung einer entsprechenden Bestimmung in § 123 Abs. 2a KFG, wodurch besonders geschulten Organen der Asfinag diese Befugnis eingeräumt wird\n\tIn § 125 KFG wird der Kreis der potentiellen § 125 –Sachverständigen erweitert und auf Personen mit Meisterprüfung im Kraftfahrzeugtechnikerhandwerk, die eine Reifeprüfung oder Berufsreifeprüfung erfolgreich bestanden haben, ausgedehnt\n\tIn § 134 Abs. 3c wird die Höhe des Organmandates für Verstöße gegen das sog. Handyverbot von 50 Euro auf 100 Euro angehoben und in § 134 Abs. 3d wird die Höhe des Organmandates für Verstöße gegen die Gurt- oder die Sturzhelmpflicht von 35 Euro auf 50 Euro angehoben\n\n".to_string()));
        assert_eq!(
            init.votes,
            vec![
                Vote {
                    party: Party {
                        name: "ÖVP".to_string(),
                        color: "#62C3D0".to_string(),
                        fraction: 71,
                        code: "V".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                        code: "S".to_string(),
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                        code: "F".to_string(),
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                        code: "G".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                        code: "N".to_string(),
                    },
                    infavor: true
                }
            ]
        );

        assert_eq!(init.accepted.unwrap(), AcceptedState::Accepted);

        assert_eq!(init.id, "XXVII/I/1954");
        assert_eq!(init.ityp, "I".to_string());

        assert_eq!(init.speeches[0].delegate_id, 52687);
        assert_eq!(init.speeches[0].opinion, Opinion::Against);

        assert_eq!(init.speeches[1].delegate_id, 5680);
        assert_eq!(init.speeches[1].opinion, Opinion::Infavor);

        assert_eq!(init.speeches[2].delegate_id, 51557);
        assert_eq!(init.speeches[2].opinion, Opinion::Against);

        assert_eq!(init.speeches[3].delegate_id, 83150);
        assert_eq!(init.speeches[3].opinion, Opinion::Infavor);

        assert_eq!(init.speeches[4].delegate_id, 5653);
        assert_eq!(
            init.speeches[4].opinion,
            Opinion::Other("Regierungsbank".into())
        );
    }

    #[tokio::test]
    async fn test_get_legis_init_a_3021_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3021")
            .await
            .unwrap();

        assert_eq!(init.topics, vec!["Arbeit".to_string()]);

        for speech in &init.speeches {
            println!("speech: {speech:?}");
        }

        assert_eq!(init.accepted, Some(AcceptedState::Accepted));
        assert_eq!(init.id, "XXVII/A/3021");
        assert_eq!(init.ityp, "A");
        assert_eq!(
            init.title,
            "Arbeitsvertragsrechts-Anpassungsgesetz, Änderung (3021/A) | Parlament Österreich"
        );

        assert_eq!(
            init.description,
            "Arbeitsvertragsrechts-Anpassungsgesetz, Änderung (3021/A)"
        );
        assert_eq!(
            init.votes,
            [
                Vote {
                    party: Party {
                        name: "ÖVP".to_string(),
                        color: "#62C3D0".to_string(),
                        fraction: 71,
                        code: "V".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                        code: "S".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                        code: "F".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                        code: "G".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                        code: "N".to_string(),
                    },
                    infavor: false
                }
            ]
        );
        let et = init.eurovoc_topics;
        assert_eq!(et, vec!["Arbeitsrecht"]);
        let et = init.other_keyword_topics;
        assert_eq!(et, vec!["Arbeitsrecht I. österreichisches"]);
    }

    #[tokio::test]
    async fn test_xx_legis_init() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XX".into()];

        let res = scraper.get_legis_init("/gegenstand/XX/I/1").await.unwrap();
    }

    // /gegenstand/XXVII/A/3945
    #[tokio::test]
    async fn test_legis_init_voted_by_name_xxvii_a_3945() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3945")
            .await
            .unwrap();

        res.hidden_votes_info.voted_by_name_data.unwrap();
        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
    }
    #[tokio::test]
    async fn test_xxvii_i_2269_legis_init_was_invisibly_declined_false() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2269")
            .await
            .unwrap();
        assert_ne!(res.accepted, Some(AcceptedState::PreDeclined));
    }

    #[tokio::test]
    async fn test_xxvii_erroring_legis_init_i_405() {
        //https://www.parlament.gv.at/gegenstand/XXVII/I/405
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/405")
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn test_xxvii_erroring_legis_init_xxvi_a_110() {
        //https://www.parlament.gv.at/gegenstand/XXVII/I/405
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVI/A/110")
            .await
            .unwrap();
        res.hidden_votes_info.pro_contra_parties.unwrap();
    }

    // test may get invalid -> legis init got voted on potentially
    #[tokio::test]
    async fn test_orf_gesetz_no_vote_at_some_point_xxvii_i_2082() {
        let scraper = AustriaScraper::default();

        // let out = scraper.get_gegenstand_info("/gegenstand/XXVII/I/2082").await.unwrap();
        let out = scraper
            .get_gegenstand_info("/gegenstand/XXVII/A/4070")
            .await
            .unwrap();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2082")
            .await
            .unwrap();

        println!("res: {res:?}");
        println!("acc: {:?}", res.accepted);

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2269")
            .await
            .unwrap();

        println!("acc: {:?}", res.accepted);

        // gegenstand/XXVII/I/2269
    }

    #[tokio::test]
    async fn test_xxvii_hidden_votes_maybe_not_in_db() {
        // https://www.parlament.gv.at/gegenstand/XXVII/A/3431
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3431")
            .await
            .unwrap();
        println!("res: {res:?}");
        res.hidden_votes_info.pro_contra_parties.unwrap();
    }

    // https://www.parlament.gv.at/gegenstand/XXVII/A/3874

    #[tokio::test]
    async fn test_xxvii_a_3874() {
        // https://www.parlament.gv.at/gegenstand/XXVII/A/3874
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3874")
            .await
            .unwrap();
        let pro_contras = res.hidden_votes_info.pro_contra_parties.unwrap();
        assert_eq!(pro_contras.pro, ["S", "F"]);
    }

    #[tokio::test]
    async fn test_dienstrecht_novelle_oevp_fail() {
        // https://www.parlament.gv.at/gegenstand/XXVII/I/2711

        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2711")
            .await
            .unwrap();
        println!("res: {res:?}");
        // let pro_contras = res.hidden_votes_info.pro_contra_parties.unwrap();
        // assert_eq!(pro_contras.pro, ["S", "F"]);
    }

    #[tokio::test]
    async fn test_xxvii_erroring_legis_init_xxvii_i_1531() {
        //https://www.parlament.gv.at/gegenstand/XXVII/I/405
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXVII".to_string()];

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/1531")
            .await
            .unwrap();
        res.hidden_votes_info.pro_contra_parties.unwrap();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        let paths = scraper.legis_paths(initiatives).unwrap();
        println!("paths len: {}", paths.len());
        for path in paths {
            if path.contains("XXVII/I/1531") {
                println!("YUHU")
            }
        }
    }

    #[tokio::test]
    async fn test_xxvii_aa_387() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXVII".to_string()];

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/AA/387")
            .await
            .unwrap();
        println!("res: {res:?}");
    }

    //      72 | XXIII  | I    |   527 | 117318417 |
    #[tokio::test]
    async fn test_xxiii_i_527() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXIII/I/527")
            .await
            .unwrap();
        println!("res: {res:?}");
    }

    // 110965 | XXVII  | I    |    50 | 117423840 |
    #[tokio::test]
    async fn test_xxvii_i_50() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/50")
            .await
            .unwrap();
        println!("res: {res:?}");
    }
    #[tokio::test]
    async fn test_xxvii_i_1858() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/1858")
            .await
            .unwrap();
        println!("res: {res:?}");
    }

    #[tokio::test]
    async fn test_xxvii_iii_835() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/III/835")
            .await
            .unwrap();
        println!("res: {res:?}");
    }
    
    #[tokio::test]
    async fn test_xxvii_a_3342() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3342")
            .await
            .unwrap();
        println!("res: {res:?}");
        // plenary_session_id:2747091
    }

    #[tokio::test]
    async fn test_xxv_uea_855() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXV/UEA/855")
            .await
            .unwrap();
        println!("res: {res:?}");
        // plenary_session_id:2747091
        // /gegenstand/XXV/UEA/855
    }
}
