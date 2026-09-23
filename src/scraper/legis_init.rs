mod extracting;
use async_trait::async_trait;
pub use extracting::*;
mod votes;

#[cfg(test)]
mod tests;
use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};
use common_scrapes::{
    AcceptedState, LegislativeInitiative, RelatedDelegate, Speech, VoteType, Voting,
};
use serde_json::Value;
use votes::extract_hidden_votes;

use crate::{
    caching::{disk_caching, get_disk_cache_parsed},
    convert_vienna_datetime_to_utc, documents,
    error::ScraperError,
    extract_plenar_documents, extract_text_from_html, person_url_to_id,
    proposals::ProposalScraper,
    speeches::extract_speeches,
    AustriaScraper, GegenstandPaths, LastUpdated, Scraper,
};

impl LastUpdated for LegislativeInitiative {
    fn last_updated(&self) -> DateTime<Utc> {
        self.updated_at.unwrap_or(chrono::Local::now().to_utc())
    }
}

#[async_trait]
pub trait LegislativeInitiatives: ProposalScraper + Sync {
    #[inline]
    async fn post_legis_payload(
        &self,
        searched_doktyps: Option<&[&str]>,
    ) -> Result<Value, ScraperError> {
        self.post_proposals_payload(searched_doktyps).await
    }

    async fn raw_legis_into_legis(&self, raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let proposal_path = proposal_path.to_string();
        for ityp in LEGIS_IGNORE_ITYPS {
            if proposal_path.contains(&format!("{ityp}/")) {
                return Err(ScraperError::NoVoteFound);
            }
        }
        match get_disk_cache_parsed::<_, _>(&proposal_path, self.cache_config(), |raw| async {
            let legis_init = self.raw_legis_into_legis(raw).await;
            let redo_secs = match (
                legis_init.as_ref(),
                self.cache_config().cache_dir() == "./test-data",
            ) {
                (Some(legis_init), false) => {
                    let dur = chrono::Local::now().to_utc() - legis_init.created_at;
                    let seconds = ((dur.num_days() as f32).powf(1.54) as i64 * 50)
                        .min(60 * 60 * 24 * 80)
                        .max(500);
                    Some(seconds as u64)
                }
                _ => None,
            };

            (legis_init, redo_secs)
        })
        .await
        {
            Some(legis_init) => Ok(legis_init),
            None => {
                let raw_proposal = self.get_gegenstand_info(&proposal_path).await?;

                if let Err(e) = disk_caching(
                    &proposal_path,
                    raw_proposal.to_string(),
                    self.cache_config(),
                )
                .await
                {
                    log::warn!("could cache raw {proposal_path}: {e:?}")
                }

                self.raw_legis_into_legis(raw_proposal)
                    .await
                    .ok_or(ScraperError::LegisInitParseError)
            }
        }
    }
}

// pub const IGNORE_ITYPS: [&str; 3] = ["J", "EUBTG", "AB"];
pub const LEGIS_IGNORE_ITYPS: [&str; 1] = ["EUBTG"];
pub const LEGIS_LAW_DETERMINATORS: [&str; 7] = ["RV", "A", "BRA", "BUA", "EBR", "GABR", "GABR13"];
pub const LEGIS_RESOLUTION_DETERMINATORS: [&str; 2] = ["A(E)", "UEA"];

pub const LEGIS_IS_PLENARY_VOTEABLE_ON_DOKTYPES: [&str; 23] = [
    "US", "ARH1", "A", "GABR", "UEA", "GABR13", "RV", "A(E)", "AA", "ARH2", "AMIN", "AVB", "URH2",
    "RVVB", "USVL", "BUA", "UEAM", "RVS", "BRH", "BRG", "PET", "BI", "AEA",
]; // MIND: there seems to be SOME, or at least a single AUB that has an actual plenary vote result XXIV/I/2455

#[async_trait]
impl LegislativeInitiatives for AustriaScraper {
    async fn raw_legis_into_legis(&self, raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        let legis_content = &raw_legis["content"];
        let status = legis_content["status"]["number"].as_u64().map(|x| x as u8);
        let ris_link = legis_content["status"]["bgbllinks"][0]["link"]
            .as_str()
            .map(ToString::to_string);

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let doktyp = legis_content["doktyp"].as_str()?.to_string();

        if LEGIS_IGNORE_ITYPS.contains(&ityp.as_str()) {
            return None;
        }
        let mut voting = None;

        let is_law = LEGIS_LAW_DETERMINATORS.contains(&doktyp.as_str());
        if is_law {
            voting = Some(Voting::Law);
        }
        let is_resolution = LEGIS_RESOLUTION_DETERMINATORS.contains(&doktyp.as_str());
        if is_resolution {
            voting = Some(Voting::Resolution);
        }
        if doktyp.as_str() == "AA" {
            voting = Some(Voting::Amendment);
        }

        if ityp.as_str() == "III" {
            voting = Some(Voting::Report);
        }

        let inr = legis_content["inr"].as_u64()?;
        let gp = legis_content["gp_code"].as_str()?.to_string();

        let mut raw_documents = legis_content["documents"].as_array();
        if raw_documents.is_none() {
            raw_documents = if let Some(preconst) = legis_content.get("preconst") {
                let preconst = &preconst.as_array()?[0];
                preconst["documents"].as_array()
            } else {
                None
            };
        }
        let mut documents = raw_documents
            .map(|raw_documents| documents(raw_documents, AustriaScraper::BASE_URL))
            .unwrap_or_else(|| Some(vec![]))?;

        let plenar_stage_documents = extract_plenar_documents(legis_content).unwrap_or_default();
        let new_law_text_files = plenar_stage_documents
            .iter()
            .filter(|doc| doc.title == "Gesetzestext")
            .cloned()
            .collect::<Vec<_>>();

        if !new_law_text_files.is_empty() {
            // https://github.com/som-es/somes/issues/118
            documents.retain(|doc| !doc.title.contains("Gesetzestext"));
            documents.extend(new_law_text_files);
        }

        let names = legis_content["names"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let related_delegates = names
            .iter()
            .flat_map(|name| {
                let delegate_id = name["url"].as_str().map(person_url_to_id)??;
                let text = name["funktext"].as_str().map(ToString::to_string);
                Some(RelatedDelegate { text, delegate_id })
            })
            .collect::<Vec<RelatedDelegate>>();

        let votes = extract_votes(legis_content).unwrap_or_default();
        let topics = extract_topics(legis_content, "topics").unwrap_or_default();
        let mut eurovoc_topics = extract_topics(legis_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics = extract_topics(legis_content, "headwords").unwrap_or_default();

        let is_urgent = other_keyword_topics
            .iter()
            .any(|x| x.contains("Dringliche"));

        let mut speeches: Vec<Speech> = extract_speeches(legis_content, true).unwrap_or_default();
        let date = extract_plenary_date(legis_content, "NR")?;
        let br_plenary_date = extract_plenary_date(legis_content, "BR");

        let nr_plenary_activity_date = NaiveDate::parse_from_str(date, "%d.%m.%Y ").ok()?;

        let br_plenary_activity_date = br_plenary_date.and_then(|br_plenary_date| {
            NaiveDate::parse_from_str(br_plenary_date, "%d.%m.%Y ").ok()
        });

        let vote_infavor = legis_content["vote"]["infavor"].as_bool();

        let mut accepted = if votes.is_empty() { None } else { vote_infavor }.map(|x| {
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
        let created_at = convert_vienna_datetime_to_utc(
            NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?,
        )?;

        let updated_at = legis_content["update"].as_str();
        let updated_at = updated_at.and_then(|updated_at| {
            convert_vienna_datetime_to_utc(
                NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap(),
            )
        });

        let requires_simple_majority =
            extract_majority_on_indicators(legis_content).unwrap_or(true);
        let (was_invisibly_declined, _) = extract_invisibly_declined(legis_content);

        let (plenary_session, hidden_votes) =
            extract_hidden_votes(legis_content, "Plenarberatungen NR", &self.cache_config).await;
        let mut hidden_votes = hidden_votes.unwrap_or_default();

        let (br_plenary_session, br_hidden_votes) = match voting {
            Some(Voting::Law) => {
                extract_hidden_votes(legis_content, "Plenarberatungen BR", &self.cache_config).await
            }
            _ => (None, None),
        };

        let references = extract_references(legis_content);

        // sometimes, the info is missing
        if (br_hidden_votes
            .as_ref()
            .map(|hv| hv.voted_by_name)
            .unwrap_or_default()
            || hidden_votes.voted_by_name)
            && !eurovoc_topics.contains(&"namentliche Abstimmung".into())
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
        if accepted.is_none() && was_invisibly_declined.is_some() {
            if was_invisibly_declined.unwrap() {
                accepted = Some(AcceptedState::PreDeclined);
            } else {
                accepted = Some(AcceptedState::Accepted);
            }

            // proposals are inversed declined warum auch immer !
            if let (Some(VoteType::ReversingProposal), Some(pro_contra)) = (
                hidden_votes.vote_stage.map(|x| x.vote_type),
                &mut hidden_votes.pro_contra_parties,
            ) {
                core::mem::swap(&mut pro_contra.contra, &mut pro_contra.pro);
                speeches = speeches
                    .into_iter()
                    .map(|speech| Speech {
                        delegate_id: speech.delegate_id,
                        delegate_display_name: speech.delegate_display_name,
                        opinion: speech.opinion.invert(),
                        spoken_text_html_urls: speech.spoken_text_html_urls,
                        speech_texts: None,
                    })
                    .collect();
            }
        };

        if accepted.is_none() && hidden_votes.was_declined.is_some() {
            if hidden_votes.was_declined.unwrap() {
                accepted = Some(AcceptedState::PreDeclined);
            } else {
                accepted = Some(AcceptedState::Accepted);
            }
        }

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

        let mut is_voteable_on = is_law;

        if LEGIS_IS_PLENARY_VOTEABLE_ON_DOKTYPES.contains(&doktyp.as_str()) {
            is_voteable_on = true;
        }

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
            documents: documents
                .into_iter()
                .map(|mut doc| {
                    doc.link = crate::document::add_base_url_to_url(&doc.link, Self::BASE_URL);
                    doc
                })
                .collect(),
            related_delegates,
            accepted,
            nr_plenary_activity_date,
            br_plenary_activity_date,
            vote_date: hidden_votes.voted_at_date,
            speeches,
            topics,
            eurovoc_topics,
            other_keyword_topics,
            created_at,
            updated_at,
            requires_simple_majority,
            proposal,
            hidden_votes_info: hidden_votes,
            br_hidden_votes_info: br_hidden_votes,
            br_plenary_session,
            plenary_session,
            references,
            status,
            ris_link,
            is_law,
            law_accepted: vote_infavor,
            is_voteable_on,
            is_urgent,
            voting,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        find_gegenstand_paths_from_rows(Self::LIST_ENTRIES, initiatives)
            .ok_or(ScraperError::LegisInitParseError)
    }
}

fn find_gegenstand_paths_from_rows(
    list_entries: &str,
    rows_initiatives: Value,
) -> Option<GegenstandPaths> {
    Some(
        rows_initiatives[list_entries]
            .as_array()?
            .iter()
            .flat_map(|gegenstand| {
                let gegenstand = gegenstand.as_array()?;
                let gp = gegenstand[0].as_str()?;
                let ityp = gegenstand[1].as_str()?;
                let inr = gegenstand[2].as_str();
                let inr = match inr {
                    Some(inr) => inr.to_string(),
                    None => gegenstand[2].as_i64()?.to_string(),
                };
                let inr = inr.trim_start_matches('0');
                Some(format!("/gegenstand/{gp}/{ityp}/{inr}"))
            })
            .collect(),
    )
}
