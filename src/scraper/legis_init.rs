mod come_into_effect;
use std::future::Future;

pub use come_into_effect::*;
mod votes;

use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::{
    AcceptedState, FileType, LegislativeInitiative, Party, Reference, RelatedDelegate, Speech,
    Vote, VoteType, Voting,
};
use serde_derive::{Deserialize, Serialize};
use serde_json::{json, Value};
use votes::extract_hidden_votes;

use crate::{
    caching::{disk_caching, get_disk_cache_json, get_disk_cache_parsed},
    documents,
    error::ScraperError,
    extract_from_gegenstand_url, extract_from_gegenstand_url_text, extract_plenar_documents,
    extract_text_from_html,
    filter::filter_name_extraction,
    person_url_to_id,
    proposals::ProposalScraper,
    speeches::extract_speeches,
    AustriaScraper, DeclineUrlGegenstand, GegenstandPaths, LastUpdated, Scraper, TextExtractor,
};

// const PARTIES: LazyLock<Vec<Party>> = LazyLock::new(|| {
//     let scraper = AustriaScraper::default();
//     pollster::block_on(scraper.get_parties()).unwrap()
// });

impl LastUpdated for LegislativeInitiative {
    fn last_updated(&self) -> chrono::NaiveDateTime {
        self.updated_at
            .unwrap_or(chrono::Local::now().naive_local())
    }
}

pub trait LegislativeInitiatives: Scraper + Sync {
    const FILTER_PATH: &'static str;

    fn legis_payload(&self, searched_doktyps: Option<&[&str]>) -> Self::Payload;

    fn post_legis_payload(
        &self,
        searched_doktyps: Option<&[&str]>,
    ) -> impl Future<Output = Result<Value, ScraperError>> {
        async move {
            let payload = self.legis_payload(searched_doktyps);
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
    }

    fn raw_legis_into_legis(
        &self,
        raw_legis: Value,
    ) -> impl Future<Output = Option<LegislativeInitiative>>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> impl Future<Output = Result<LegislativeInitiative, ScraperError>> {
        async move {
            for ityp in LEGIS_IGNORE_ITYPS {
                if proposal_path.contains(&format!("{ityp}/")) {
                    return Err(ScraperError::NoVoteFound);
                }
            }
            match get_disk_cache_parsed::<_>(proposal_path, 60 * 60 * 24 * 190, async |raw| {
                let legis_init = self.raw_legis_into_legis(raw).await;
                let redo_secs = if let Some(legis_init) = legis_init.as_ref() {
                    let dur = chrono::Local::now().date_naive() - legis_init.created_at.date();
                    let seconds = ((dur.num_days() as f32).powf(1.54) as i64 * 50)
                        .min(60 * 60 * 24 * 400)
                        .max(500);
                    Some(seconds as u64)
                } else {
                    None
                };

                (legis_init, redo_secs)
            })
            .await
            {
                Some(legis_init) => Ok(legis_init),
                None => {
                    log::info!("NO CACHE!!!!!!!! {proposal_path:?}");

                    let raw_proposal = self.get_gegenstand_info(proposal_path).await?;

                    if let Err(e) = disk_caching(proposal_path, raw_proposal.to_string()).await {
                        log::warn!("could cache raw {proposal_path}: {e:?}")
                    }

                    self.raw_legis_into_legis(raw_proposal)
                        .await
                        .ok_or(ScraperError::LegisInitParseError)
                }
            }
        }
    }
}

// pub const IGNORE_ITYPS: [&str; 3] = ["J", "EUBTG", "AB"];
pub const LEGIS_IGNORE_ITYPS: [&str; 1] = ["EUBTG"];
pub const LEGIS_LAW_DETERMINATORS: [&str; 7] = ["RV", "A", "BRA", "BUA", "EBR", "GABR", "GABR13"];
pub const LEGIS_AMENDMENT_DETERMINATORS: [&str; 2] = ["A(E)", "UEA"];

pub const LEGIS_IS_VOTEABLE_ON_DOKTYPES: [&str; 17] = [
    "US", "ARH1", "A", "GABR", "UEA", "GABR13", "RV", "A(E)", "AA", "ARH2", "AMIN", "AVB", "URH2",
    "RVVB", "USVL", "BUA", "UEAM",
];

// #[async_trait]
impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/filter/data/101?js=eval&showAll=true&export=true";
    // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=undefined&showAll=true&export=true";
    // "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=undefined&showAll=true&export=true";

    fn legis_payload(&self, searched_doktyps: Option<&[&str]>) -> Self::Payload {
        json!({
            "GP": self.scrape_info.gp,
            "NRBR": self.scrape_info.council_type,
            "DOKTYP": searched_doktyps,
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
            /*"VHG": [ // voteable ons
                "ANTR",
                "RV",
                "RVVB",
                "GABR",
                "GABR13",
                "US",
            ],*/
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

    async fn raw_legis_into_legis(&self, raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        let legis_content = &raw_legis["content"];
        let status = legis_content["status"]["number"].as_u64().map(|x| x as u8);

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let doktyp = legis_content["doktyp"].as_str()?.to_string();

        if LEGIS_IGNORE_ITYPS.contains(&ityp.as_str()) {
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
        let mut documents = documents(raw_documents)?;
        documents.extend(extract_plenar_documents(legis_content).unwrap_or_default());

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
        let date = extract_date(legis_content)?;

        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y ").ok()?;

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
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;

        let updated_at = legis_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let requires_simple_majority =
            extract_majority_on_indicators(legis_content).unwrap_or(true);
        let (was_invisibly_declined, _) = extract_invisibly_declined(legis_content);

        let (plenary_session, hidden_votes) = extract_hidden_votes(legis_content, &ityp).await;
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

        let mut voting = None;

        let is_law = LEGIS_LAW_DETERMINATORS.contains(&doktyp.as_str());
        if is_law {
            voting = Some(Voting::Law);
        }
        let is_amendment = LEGIS_AMENDMENT_DETERMINATORS.contains(&doktyp.as_str());
        if is_amendment {
            voting = Some(Voting::Amendment);
        }
        if doktyp.as_str() == "AA" {
            voting = Some(Voting::Resolution);
        }

        let mut is_voteable_on = is_law;

        if LEGIS_IS_VOTEABLE_ON_DOKTYPES.contains(&&doktyp.as_str()) {
            is_voteable_on = true;
        }

        let mut come_into_effect_info = None;
        if vote_infavor == Some(true) {
            // search text for "in Kraft getreten" oder so
            let law_file = documents
                .iter()
                .find(|x| x.title.contains("Gesetz") && x.ftype == FileType::HTML);
            if let Some(law_file) = law_file {
                let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);
                let text = text_extractor
                    .extract_text_from_url(law_file, None, true)
                    .await;
                if let Some(text) = text {
                    come_into_effect_info = come_into_effect::extract_come_into_effect(&text);
                }

                // let url = format!("{}{}", AustriaScraper::BASE_URL, law_file.link);
                // println!("{id}: doktyp: {doktyp} law_file: yes!: {url:?}");
            }
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
            documents,
            related_delegates,
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
            status,
            is_law,
            law_accepted: vote_infavor,
            come_into_effect_info,
            is_voteable_on,
            is_urgent,
            voting,
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

pub fn extract_invisibly_declined_from_text(
    text: &str,
) -> (Option<bool>, Option<DeclineUrlGegenstand>) {
    let url = extract_from_gegenstand_url_text(text);
    if text.contains("abgelehnt") {
        return (Some(true), url);
    }

    if text.contains("angenommen") || text.contains("Nationalrates: Genehmigung") {
        return (Some(false), url);
    }

    (None, None)
}

pub fn extract_invisibly_declined(
    legis_content: &Value,
) -> (Option<bool>, Option<DeclineUrlGegenstand>) {
    let status_description = legis_content["status"]["description"].as_str();

    let Some(status_description) = status_description else {
        return (None, None);
    };
    return extract_invisibly_declined_from_text(status_description);
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
    use std::collections::HashSet;

    use chrono::{Days, NaiveDate};
    use common_scrapes::{
        AcceptedState, FileType, Opinion, Party, ProContra, Stage, VoteStage, VoteType,
    };

    use crate::{
        extract_text_from_html,
        legis_init::{votes::extract_vote_count, LegislativeInitiatives, Vote},
        AustriaScraper, Scraper,
    };

    use super::is_accepted_based_on_votes;

    #[tokio::test]
    pub async fn test_xxvii_a_4098() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/4098")
            .await
            .unwrap();

        let cie = init.come_into_effect_info.unwrap();
        assert_eq!(cie.by_publication, false);
        assert_eq!(
            cie.law_come_into_effect_date,
            Some(NaiveDate::from_ymd_opt(2024, 7, 1).unwrap())
        );
        assert_eq!(cie.law_expires_on_date, None);
    }

    #[tokio::test]
    pub async fn test_xxvii_a_3762() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3762")
            .await
            .unwrap();

        assert!(init.is_law);
        let cie = init.come_into_effect_info.unwrap();
        assert_eq!(cie.by_publication, true);
        assert_eq!(cie.law_come_into_effect_date, None);
        assert_eq!(
            cie.law_expires_on_date,
            Some(NaiveDate::from_ymd_opt(2027, 12, 31).unwrap())
        );
    }

    #[tokio::test]
    pub async fn test_xxvii_a_2173() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/2173")
            .await
            .unwrap();

        assert!(init.is_law);
        // let cie = init.come_into_effect_info.unwrap();
        // assert_eq!(cie.by_publication, true);
        // assert_eq!(cie.law_come_into_effect_date, None);
        // assert_eq!(
        //     cie.law_expires_on_date,
        //     Some(NaiveDate::from_ymd_opt(2027, 12, 31).unwrap())
        // );
    }

    // /gegenstand/XXVIII/I/22
    #[tokio::test]
    pub async fn test_xxviii_i_22() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVIII/I/22")
            .await
            .unwrap();

        assert!(init.is_law);
        let cie = init.come_into_effect_info.unwrap();
        assert_eq!(cie.by_publication, true);
    }

    #[test]
    fn test_vote_count_extraction() {
        let text = r#"<a href=\"/gegenstand/XXV/NRSITZ/75\">75. Sitzung des Nationalrates</a>: Unselbständiger Entschließungsantrag abgelehnt<br>Namentliche Abstimmung (abgegebene Stimme: 141, davon Ja-Stimmen: 34, Nein-Stimmen: 107)"#;
        let x = extract_vote_count(text).unwrap();
        assert_eq!(x.0, 141);
        assert_eq!(x.1, 34);
        assert_eq!(x.2, 107);
    }

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
        // assert_eq!("Schwerpunkte der Regierungsvorlage\\n\\n\\n\\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\\n\\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\\n\\n", text);
    }

    // /gegenstand/XXVIII/A/104
    #[tokio::test]
    async fn test_unfinished_xxviii_a_104() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVIII/A/104")
            .await
            .unwrap();
        println!("init: {init:?}");
        assert!(init.is_voteable_on);
    }

    #[tokio::test]
    async fn test_legis_init_all() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XXVIII".to_string()];

        scraper.scrape_info.date_start = chrono::Local::now()
            .naive_local()
            .checked_sub_days(Days::new(90));

        let initiatives = scraper.post_legis_payload(None).await.unwrap();
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
            // if path.contains("I/1858") {
            //     println!("path: {path:?}");
            // }
            if path.contains("A/77") {
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
        assert_eq!(
            legis_init
                .related_delegates
                .iter()
                .map(|x| x.delegate_id)
                .collect::<Vec<_>>(),
            vec![5439, 87001]
        );
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
            legis_init.hidden_votes_info.vote_stage.unwrap().stage,
            Stage::Second
        );
    }

    #[tokio::test]
    async fn test_get_legis_init_with_accepted_and_status_5() {
        let mut scraper = AustriaScraper::default();
        return;

        let start = std::time::Instant::now();

        let mut count = 0;
        // for gp in ["XXIV", "XXIII", "XXII"] {
        for gp in [
            "XXVIII", "XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX",
        ] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            if gp != "XXVII" {
                continue;
            }

            let initiatives = scraper.post_legis_payload(None).await.unwrap();
            let mut doktypes_with_law = HashSet::new();
            for path in scraper.legis_paths(initiatives).unwrap() {
                // println!();
                // println!("path: {path:?}");
                let Ok(init) = scraper.get_legis_init(&path).await else {
                    // println!("error {}{path}", AustriaScraper::BASE_URL);
                    continue;
                };

                let law_file = init
                    .documents
                    .iter()
                    .find(|x| x.title.contains("Gesetz") && x.ftype == FileType::HTML);
                if let Some(_law_file) = law_file {
                    if init.doktyp == "AUB" {
                        // let url = format!("{}/gegenstand/{}/{}/{}", AustriaScraper::BASE_URL, init.gp, init.ityp, init.inr);
                        // println!("url: {url:?}")
                    }
                    doktypes_with_law.insert(init.doktyp);
                }

                if init.is_law
                /*&& init.law_accepted == Some(true)*/
                {
                    count += 1;
                }
            }
            println!("elapsed: {:?}", start.elapsed());
            println!("count: {count:?}");
            println!("{doktypes_with_law:?}");
            // break;
        }

        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_was_voted_by_name() {
        let mut scraper = AustriaScraper::default();

        return;
        let start = std::time::Instant::now();

        let mut doktypes = HashSet::new();
        let mut itypes = HashSet::new();
        let mut count = 0;
        // for gp in ["XXIV", "XXIII", "XXII"] {
        for gp in [
            // "XXVIII", "XXVII", "XXVI", "XXV", "XXIV",
            "XXIII", "XXII", "XXI", "XX",
        ] {
            scraper.scrape_info.gp = vec![gp.to_string()];

            let initiatives = scraper.post_legis_payload(None).await.unwrap();
            for path in scraper.legis_paths(initiatives).unwrap() {
                // println!();
                // println!("path: {path:?}");
                let Ok(init) = scraper.get_legis_init(&path).await else {
                    println!("error {}{path}", AustriaScraper::BASE_URL);
                    continue;
                };
                doktypes.insert(init.doktyp.clone());
                itypes.insert(init.ityp.clone());

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
            if gp == "XXIV" {
                break;
            }
        }

        println!("doktypes: {doktypes:?}");
        println!("itypes: {itypes:?}");
        /*
               doktypes: {"US", "ARH1", "A", "GABR", "UEA", "GABR13", "RV", "A(E)", "AA", "ARH2", "AMIN", "AVB", "URH2", "RVVB", "USVL", "BUA", "UEAM"}
        itypes: {"AA", "URH2", "UEA", "I", "US", "A"}
                 */

        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_was_invisibly_declined() {
        return;
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload(None).await.unwrap();
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
                    ty = init.hidden_votes_info.vote_stage
                );
                count += 1;
            }
        }

        println!("elapsed: {:?}", start.elapsed());
        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_majority() {
        return;
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload(None).await.unwrap();
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

        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );

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

        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
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

        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );

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
        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );

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

        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
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
        let named_vote = init.hidden_votes_info.clone().voted_by_name_data.unwrap();
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
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
        // assert_eq!(init.emphasis, Some("Schwerpunkte der Regierungsvorlage\n\n\n\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\n\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\n\n \n\tNeuregelung der Ausbildung und Prüfung des in den Fahrschulen eingesetzten Lehrpersonals, wobei ein stärkerer Praxisbezug hergestellt wird und auch eine verpflichtende Aus- und Weiterbildung vorgeschrieben wird\n\tSchaffung einer entsprechenden Bestimmung in § 123 Abs. 2a KFG, wodurch besonders geschulten Organen der Asfinag diese Befugnis eingeräumt wird\n\tIn § 125 KFG wird der Kreis der potentiellen § 125 –Sachverständigen erweitert und auf Personen mit Meisterprüfung im Kraftfahrzeugtechnikerhandwerk, die eine Reifeprüfung oder Berufsreifeprüfung erfolgreich bestanden haben, ausgedehnt\n\tIn § 134 Abs. 3c wird die Höhe des Organmandates für Verstöße gegen das sog. Handyverbot von 50 Euro auf 100 Euro angehoben und in § 134 Abs. 3d wird die Höhe des Organmandates für Verstöße gegen die Gurt- oder die Sturzhelmpflicht von 35 Euro auf 50 Euro angehoben\n\n".to_string()));
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

        // let initiatives = scraper.post_legis_payload().await.unwrap();
        // let paths = scraper.legis_paths(initiatives).unwrap();
        // println!("paths len: {}", paths.len());
        // for path in paths {
        //     if path.contains("XXVII/I/1531") {
        //         println!("YUHU")
        //     }
        // }
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

        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::AnyNamedVote,
                stage: Stage::None
            }
        );

        assert_eq!(res.accepted, Some(AcceptedState::Declined));
        assert_eq!(res.plenary_session, Some(257));
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 127);
        let pro_contra = named_vote.delegates_pro_contra.unwrap();
        assert!(pro_contra.contra.contains(&"Reimon".to_string()));
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

        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::Proposal,
                stage: Stage::None
            }
        );
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.as_ref().unwrap(),
            &ProContra {
                pro: vec![
                    "V".to_string(),
                    "S".to_string(),
                    "G".to_string(),
                    "N".to_string()
                ],
                contra: vec!["F".into()]
            }
        );
        println!("res: {res:?}");
    }

    #[tokio::test]
    async fn test_xxvii_a_3342() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3342")
            .await
            .unwrap();

        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::ReversingProposal,
                stage: Stage::None
            }
        );
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.as_ref().unwrap(),
            &ProContra {
                contra: vec!["V".to_string(), "S".to_string(), "G".to_string()],
                pro: vec!["F".into(), "N".into()]
            }
        );
        println!("res: {res:?}");
        // plenary_session_id:2747091
    }

    // THIS HAS WRONG PRO AND CONTRA COUNT - not anymore!!
    #[tokio::test]
    async fn test_xxv_uea_855() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXV/UEA/855")
            .await
            .unwrap();
        println!("res: {res:?}");

        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::AnyNamedVote,
                stage: Stage::None
            }
        );

        assert_eq!(res.accepted, Some(AcceptedState::Declined));
        assert_eq!(res.plenary_session, Some(179));
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 122);
        let pro_contra = named_vote.delegates_pro_contra.unwrap();
        assert!(pro_contra.pro.contains(&"Wurm Peter".to_string()));
        // plenary_session_id:2747091
        // /gegenstand/XXV/UEA/855
    }

    // /gegenstand/XXV/UEA/835?

    #[tokio::test]
    async fn test_xxv_uea_835() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXV/UEA/835")
            .await
            .unwrap();
        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Declined));
        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::AnyNamedVote,
                stage: Stage::None
            }
        );
        assert_eq!(res.plenary_session, Some(171));
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 126);
        let pro_contra = named_vote.delegates_pro_contra.unwrap();
        assert!(pro_contra.contra.contains(&"Alm".to_string()));
        // plenary_session_id:2747091
        // /gegenstand/XXV/UEA/855
    }

    #[tokio::test]
    async fn test_xxvii_a_2956() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/2956")
            .await
            .unwrap();

        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::ReversingProposal,
                stage: Stage::None
            }
        );
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.as_ref().unwrap(),
            &ProContra {
                contra: vec![
                    "V".to_string(),
                    "S".to_string(),
                    "G".to_string(),
                    "N".into()
                ],
                pro: vec!["F".into()]
            }
        );
    }

    #[tokio::test]
    async fn test_xxvii_a_571() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVI/A/571")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::Proposal,
                stage: Stage::None
            }
        );
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.as_ref().unwrap(),
            &ProContra {
                pro: vec!["S".to_string(), "N".to_string(), "J".to_string()],
                contra: vec!["V".into(), "F".into()]
            }
        );
    }

    #[tokio::test]
    async fn test_xxvii_uea_144() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVI/UEA/144")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Declined));
        assert_eq!(
            res.hidden_votes_info.vote_stage.unwrap(),
            VoteStage {
                vote_type: VoteType::AnyNamedVote,
                stage: Stage::None
            }
        );
        assert_eq!(res.plenary_session, Some(60));
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.as_ref().unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 109);
        let pro_contra = named_vote.delegates_pro_contra.unwrap();
        assert!(pro_contra.contra.contains(&"Zanger Wolfgang".to_string()));
    }

    #[tokio::test]
    async fn test_xxvi_i_282() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVI/I/282")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, None);
    }

    #[tokio::test]
    async fn test_xxvi_i_178() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVI/I/178")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, None);
    }

    #[tokio::test]
    async fn test_xxv_uea_443() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXV/UEA/443")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted.unwrap(), AcceptedState::Declined);
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();
        assert!(named_vote
            .delegates_pro_contra
            .as_ref()
            .unwrap()
            .contra
            .contains(&"Yilmaz".to_string()));

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 83);
        assert_eq!(named_vote.pro_count, 52);
        assert_eq!(named_vote.vote_sum, 135);
    }

    #[tokio::test]
    async fn test_xxiv_uea_1019() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXIV/UEA/1019")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted.unwrap(), AcceptedState::Declined);
        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();
        assert!(named_vote
            .delegates_pro_contra
            .as_ref()
            .unwrap()
            .contra
            .contains(&"Wurm".to_string()));

        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 98);
        assert_eq!(named_vote.pro_count, 63);
        assert_eq!(named_vote.vote_sum, 161);
    }

    // gegenstand/XXVII/I/2704
    #[tokio::test]
    async fn test_xxvii_i_2704() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2704")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, None);
    }

    //
    #[tokio::test]
    async fn test_xxii_uea_373() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXII/UEA/373")
            .await
            .unwrap();

        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        println!("res: {res:?}");

        let named_vote = res.hidden_votes_info.voted_by_name_data.unwrap();

        assert!(named_vote
            .delegates_pro_contra
            .as_ref()
            .unwrap()
            .contra
            .contains(&"Krainer".to_string()));
        assert!(named_vote
            .delegates_pro_contra
            .as_ref()
            .unwrap()
            .contra
            .contains(&"Zinggl".to_string()));
        assert_eq!(
            named_vote.contra_count,
            named_vote
                .delegates_pro_contra
                .as_ref()
                .unwrap()
                .contra
                .len()
        );
        assert_eq!(
            named_vote.pro_count,
            named_vote.delegates_pro_contra.unwrap().pro.len()
        );
        assert_eq!(named_vote.contra_count, 82);
        assert_eq!(named_vote.pro_count, 91);
        assert_eq!(named_vote.vote_sum, 173);
    }

    // /gegenstand/XVIII/UEA/8

    #[tokio::test]
    async fn test_xvviii_uea_8() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/UEA/8")
            .await
            .unwrap();

        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        println!("res: {res:?}");
    }
    // gegenstand/XXII/E/97
    #[tokio::test]
    async fn test_xxii_e_97() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXII/E/97")
            .await
            .unwrap();

        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        println!("res: {res:?}");
    }

    #[tokio::test]
    async fn test_xxviii_i_26() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/I/26")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().contra,
            Vec::<String>::new()
        );
    }

    #[tokio::test]
    async fn test_xxvii_uea_1266() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/UEA/1266")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().pro,
            vec!["S".to_string()]
        );
    }

    // gegenstand/XXVIII/AA/6
    #[tokio::test]
    async fn test_xxviii_aa_6() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/AA/6")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().pro,
            vec!["V".into(), "S".to_string(), "N".into()]
        );
    }

    // gegenstand/XXVIII/III/79
    #[tokio::test]
    async fn test_xxviii_iii_79() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/III/79")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().pro,
            vec![
                "F".into(),
                "V".into(),
                "S".to_string(),
                "N".into(),
                "G".into()
            ]
        );
    }

    #[tokio::test]
    async fn test_xxviii_a_77() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/A/77")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::Accepted));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().pro,
            vec!["F".into(), "V".into(), "S".to_string(), "N".into(),]
        );
    }
    // XXVII/A/769
    #[tokio::test]
    async fn test_wrong_refs_xxvii_a_769() {
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/769")
            .await
            .unwrap();

        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().pro,
            vec!["S".to_string()]
        );
    }

    // XXVII/A/2238
    #[tokio::test]
    async fn test_xxvii_a_2238() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/2238")
            .await
            .unwrap();
        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert_eq!(
            res.hidden_votes_info
                .pro_contra_parties
                .as_ref()
                .unwrap()
                .pro,
            vec!["S".to_string()]
        );
        assert_eq!(
            res.hidden_votes_info.pro_contra_parties.unwrap().contra,
            vec![
                "V".to_string(),
                "F".to_string(),
                "G".to_string(),
                "N".to_string()
            ]
        );
    }

    // XXVIII/J/439
    #[tokio::test]
    async fn test_xxviii_j_439() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/J/439")
            .await
            .unwrap();
        println!("res: {res:?}");
        assert_eq!(res.accepted, None);
        assert!(res.is_urgent);
    }
    // https://www.parlament.gv.at/gegenstand/XXVII/A/110?selectedStage=100
    #[tokio::test]
    async fn test_xxvii_a_110() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/110")
            .await
            .unwrap();
        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert!(res.is_urgent);
    }

    //XXVII/A/769
    #[tokio::test]
    async fn test_xxvii_a_769() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/769")
            .await
            .unwrap();
        println!("res: {res:?}");
        assert_eq!(res.accepted, Some(AcceptedState::PreDeclined));
        assert!(res.is_urgent);
        assert_eq!(
            res.hidden_votes_info
                .pro_contra_parties
                .as_ref()
                .unwrap()
                .pro,
            vec!["S".to_string()]
        );
    }

    #[tokio::test]
    async fn test_named_vote_xxviii_i_136() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVIII/I/136")
            .await
            .unwrap();
        println!("res: {res:?}");
        let pro_contra = res.hidden_votes_info.voted_by_name_data.unwrap().delegates_pro_contra.unwrap();
        for pro_checks in ["Auer", "Baumann", "Deckenbacher", "Dengler", "Doppelbauer", "Eder", "Zopf", "Zarits"] {
            assert!(pro_contra.pro.contains(&pro_checks.to_string()));
        }
        
        for contra_checks in ["Belakowitsch", "Darmann", "Fuchs", "Nemeth", "Voglauer", "Zanger", "Zorba"] {
            assert!(pro_contra.contra.contains(&contra_checks.to_string()));
        }
    }
}
