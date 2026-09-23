use super::EuParliamentScraper;
use crate::{
    error::ScraperError,
    eu_parliament::votes::{vot_parsers::VoteType, Votes},
    legis_init::LegislativeInitiatives,
    proposals::ProposalScraper,
};
use async_trait::async_trait;
use common_scrapes::{
    AcceptedState, DelegateRef, HiddenVotes, LegislativeInitiative, NamedVoteProContra, Proposal,
    VotedByName,
};
use serde_derive::{Deserialize, Serialize};
use serde_json::Value;

/// A legislative procedure bundled together with whatever roll-call vote(s) decided
/// it — the EU-side equivalent of AT's `LegislativeInitiative`, which embeds
/// `votes: Vec<Vote>` directly alongside its `proposal` field so callers never have to
/// separately fetch a proposal and its votes and correlate them by hand.
///
/// Unlike `LegislativeInitiative`, this isn't a `common_scrapes` type: the EU
/// proposal/vote link has to be resolved after the fact (see
/// `EuParliamentScraper::get_votes_for_proposal`) rather than being embedded by the
/// source API, and a `Voteable` makes no assumption that its proposal has been voted on
/// at all — `votes` is simply empty for a proposal that exists but hasn't (yet) gone
/// through a recorded roll-call vote.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Voteable {
    pub proposal: Proposal,
    pub votes: Votes,
}

impl Voteable {
    #[inline]
    pub fn has_votes(&self) -> bool {
        !self.votes.votes.is_empty()
    }
}

impl EuParliamentScraper {
    pub async fn get_voteable(&self, process_id: &str) -> Result<Voteable, ScraperError> {
        let proposal = self.get_proposal(process_id).await?;
        let votes = self.get_votes_for_proposal(process_id).await?;
        Ok(Voteable { proposal, votes })
    }
}

#[async_trait]
impl LegislativeInitiatives for EuParliamentScraper {
    #[inline]
    async fn post_legis_payload(
        &self,
        searched_doktyps: Option<&[&str]>,
    ) -> Result<Value, ScraperError> {
        self.post_proposals_payload(searched_doktyps).await
    }

    async fn raw_legis_into_legis(&self, raw_legis: Value) -> Option<LegislativeInitiative> {
        unimplemented!()
    }

    fn legis_paths(&self, initiatives: Value) -> Result<Vec<String>, ScraperError> {
        self.proposal_paths(initiatives)
    }

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let proposal_path = proposal_path.to_string();
        let mut voteable = self.get_voteable(&proposal_path).await?;

        voteable
            .votes
            .votes
            .sort_by(|a, b| b.sitting_date.cmp(&a.sitting_date));

        let main_vote = voteable
            .votes
            .votes
            .iter()
            .find(|vote| vote.is_main.unwrap_or_default());
        if let Some(Some(meta)) = main_vote.map(|vote| &vote.meta) {
            if meta.abstention_count.is_none()
                || meta.against_count.is_none()
                || meta.against_count.is_none()
            {
                return Err(ScraperError::NoVoteFound);
            }
        }

        Ok(LegislativeInitiative {
            proposal: voteable.proposal.clone(),
            id: voteable.proposal.id,
            ityp: voteable.proposal.ityp,
            doktyp: "".into(),
            inr: voteable.proposal.inr,
            title: voteable.proposal.title.clone(),
            description: voteable.proposal.title,
            emphasis: None,
            gp: voteable.proposal.gp, // TODO: find proper GP (consider via votes/sitting dates doc)
            votes: vec![],
            accepted: if main_vote.is_some() {
                Some(AcceptedState::Unknown)
            } else {
                None
            },
            documents: voteable.proposal.documents,
            related_delegates: vec![],
            nr_plenary_activity_date: voteable
                .proposal
                .updated_at
                .map(|date| date.date_naive())
                .unwrap_or(voteable.proposal.created_at.date_naive()),
            br_plenary_activity_date: None,
            vote_date: main_vote.map(|vote| vote.sitting_date),
            speeches: vec![],
            topics: vec![],
            eurovoc_topics: vec![],
            other_keyword_topics: vec![],
            created_at: voteable.proposal.created_at,
            updated_at: voteable.proposal.updated_at,
            requires_simple_majority: true,
            hidden_votes_info: main_vote
                .map(|vote| HiddenVotes {
                    vote_stage: None,
                    pro_contra_parties: None,
                    voted_by_name: true,
                    voted_by_name_data: vote.roll_call_vote.as_ref().map(|roll_call_vote| {
                        let mut pro_contra = NamedVoteProContra::default();
                        for vote in &roll_call_vote.votes {
                            match vote.choice {
                                super::votes::EuVoteChoice::For => {
                                    pro_contra.pro.push(DelegateRef {
                                        name: "".into(),
                                        id: Some(vote.delegate_id as u32),
                                    })
                                }
                                super::votes::EuVoteChoice::Against => {
                                    pro_contra.contra.push(DelegateRef {
                                        name: "".into(),
                                        id: Some(vote.delegate_id as u32),
                                    })
                                }
                                super::votes::EuVoteChoice::Abstention => {
                                    pro_contra.abstention.push(DelegateRef {
                                        name: "".into(),
                                        id: Some(vote.delegate_id as u32),
                                    })
                                }
                            }
                        }
                        VotedByName {
                            delegates_pro_contra: Some(pro_contra),
                            vote_sum: (roll_call_vote.abstention_count
                                + roll_call_vote.favor_count
                                + roll_call_vote.against_count)
                                as usize,
                            abstention_count: Some(roll_call_vote.abstention_count as usize),
                            no_vote_count: None,
                            pro_count: roll_call_vote.favor_count as usize,
                            contra_count: roll_call_vote.against_count as usize,
                            invalid_count: 0,
                        }
                    }),
                    was_declined: None,
                    voted_at_date: Some(vote.sitting_date),
                    voted_unanimously: false,
                })
                .unwrap_or_default(),
            br_hidden_votes_info: None,
            br_plenary_session: None,
            plenary_session: main_vote.map(|vote| {
                vote.sitting_date
                    .format("%Y%m%d")
                    .to_string()
                    .parse::<u64>()
                    .unwrap_or(0)
            }),
            references: Some(voteable.votes.references),
            status: None,
            ris_link: None,
            is_law: false,
            law_accepted: None,
            is_voteable_on: true,
            is_urgent: false,
            voting: main_vote
                .and_then(|vote| vote.meta.as_ref().and_then(|meta| meta.vote_type.as_ref()))
                .and_then(|vote_type: &VoteType| match vote_type {
                    VoteType::Report => Some(common_scrapes::Voting::Report),
                    VoteType::Resolution => Some(common_scrapes::Voting::Resolution),
                    VoteType::Proposal => Some(common_scrapes::Voting::Law),
                    _ => None,
                }),
        })
    }
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;

    use crate::caching::CacheConfig;

    use super::*;

    #[tokio::test]
    async fn test_2025_0429_legis_init() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_legis_init("2025-0429").await.unwrap();
        dbg!(&voteable.title);
        dbg!(&voteable.documents);
        let voted_by_name = voteable.hidden_votes_info;
        // dbg!(&voted_by_name);
    }

    #[tokio::test]
    async fn test_2025_0385_legis_init() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_legis_init("2025-0385").await.unwrap();
        let voted_by_name = voteable.hidden_votes_info;
        dbg!(&voted_by_name);
    }

    #[tokio::test]
    async fn test_eu_get_legis_init() {
        let scraper = EuParliamentScraper::default();
        let legis_init = scraper.get_legis_init("2024-0003").await.unwrap();

        assert_eq!(legis_init.proposal.ityp, "BUD");
        assert_eq!(legis_init.proposal.inr, 20240003);
        dbg!(legis_init);
    }

    #[tokio::test]
    async fn test_get_voteable_with_vote() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2024-0003").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "BUD");
        assert!(voteable.has_votes());
        assert_eq!(voteable.votes.votes.len(), 1);
        assert_eq!(
            voteable.votes.votes[0]
                .meta
                .as_ref()
                .unwrap()
                .favor_count
                .unwrap(),
            547
        );
    }

    #[tokio::test]
    async fn test_get_voteable_for_procedure_missing_plenary_vote_results_event() {
        let scraper = EuParliamentScraper::default();

        // 2024-0021 ("gradual roll-out of Eudamed") is the regression case for a bug
        // in the old vote-linking approach: its own procedure timeline has no
        // `PLENARY_VOTE_RESULTS` event, only bare `PLENARY_VOTE`, yet it really was
        // decided by roll-call vote 168997 on 2024-04-25 (511/1/21). See
        // `votes::tests::test_get_votes_for_proposal_missing_plenary_vote_results_event`
        // and docs/eu-parliament-scraper.md.
        let voteable = scraper.get_voteable("2024-0021").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "COD");
        assert!(voteable.has_votes());
        // assert_eq!(voteable.votes.len(), 1);
        // assert_eq!(voteable.votes[0].meta.voting_id, 168997);
        // assert_eq!(voteable.votes[0].meta.favor_count, Some(511));
    }

    #[tokio::test]
    async fn test_get_voteable_committee_report_reference_format() {
        let scraper = EuParliamentScraper::default();

        let voteable = scraper.get_voteable("2025-2211").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "INI");
        assert!(voteable.has_votes());
        assert_eq!(voteable.votes.votes.len(), 1);
        /*
        * [
            "2025/2211(INI)",
            "A10-0167/2026",
            "TA10-0270/2026",
        ]
        *
        */
        dbg!(&voteable.proposal);
        let meta = voteable.votes.votes[0].meta.as_ref().unwrap();
        dbg!(&meta);
        assert_eq!(meta.voting_id, 194869);
        assert_eq!(meta.favor_count, Some(366));
        assert_eq!(meta.against_count, Some(192));
        assert_eq!(meta.abstention_count, Some(39));
    }
    #[tokio::test]
    async fn test_cpm_with_htv() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        let howtheyvote_data = parse_howtheyvote(&scraper).await;
        let entries = howtheyvote_data
            .iter()
            .filter(|entry| entry.procedure_reference.is_some() && entry.is_main)
            .collect::<Vec<_>>();

        for entry in entries {
            let reference = entry.procedure_reference.as_ref().unwrap();
            let reference_regex = Regex::new(r"(\d{4})/(\d{4})([A-Z]?)?\([A-Z]+\)").unwrap();

            let procedure = reference_regex
                .captures_iter(reference)
                .filter_map(|caps| {
                    let year = caps.get(1)?.as_str();
                    let number = caps.get(2)?.as_str();
                    let mut prod_id = format!("{year}-{number}");
                    if let Some(ch) = caps.get(3) {
                        if !ch.is_empty() {
                            prod_id.push(ch.as_str().chars().next().unwrap());
                        }
                    }
                    Some(prod_id)
                })
                .next()
                .unwrap();

            // if procedure != "2023-0212" {
            //     continue;
            // }

            loop {
                match scraper.get_voteable(&procedure).await {
                    Err(e) => {
                        println!("failed on procedure: {procedure:?} because of {e:?}");
                        if let ScraperError::TooManyRequests(secs) = e {
                            tokio::time::sleep(secs.unwrap()).await;
                            println!("retrying {procedure:?} ");
                            continue;
                        }
                        break;
                    }
                    Ok(e) => {
                        if !e
                            .votes
                            .votes
                            .iter()
                            .any(|vote| vote.is_main.unwrap_or_default())
                        {
                            println!("procedure DOES NOT HAVE MAIN VOTE HOWEVER IT REQUIRES IT :( {reference:?} prod: {procedure} {}", entry.id);
                        }
                        break;
                    }
                }
            }
        }
    }

    // 2020/0157M
    #[tokio::test]
    async fn test_2020_0157m() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2020-0157M").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2025-0429
    #[tokio::test]
    async fn test_2025_0429() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2025-0429").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2023-0453
    #[tokio::test]
    async fn test_2023_0453() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2023-0453").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2024-0258
    #[tokio::test]
    async fn test_2024_0258() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2024-0258").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    //  2020-2151
    #[tokio::test]
    async fn test_2020_2151() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let scraper = EuParliamentScraper::default();
        // laut How they vote => A9-0094/2021, aber => Some("Décharge 2019 : Fondation européenne pour l'amélioration des conditions de vie et de travail - 2019 discharge: European Foundation for the Improvement of Living and Working Conditions - Entlastung 2019: Europäische Stiftung zur Verbesserung der Lebens- und Arbeitsbedingungen - A9-0074/2021 - Joachim Stanisław Brudziński - Propositions de décision",
        let voteable = scraper.get_voteable("2020-2151").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_2020_2086() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2020-2086").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_2020_2818() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2020-2818").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        let val = voteable.votes.votes.last().unwrap();
        dbg!(val);
        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }
    // 2020-2115
    #[tokio::test]
    async fn test_2020_2115() {
        let scraper = EuParliamentScraper::default();
        //  &roll.description = Some(
        //     "A-0269/2020 - Reinhard Bütikofer - Motion for a resolution 21/01/2021 12:20:53.337",
        // )
        let voteable = scraper.get_voteable("2020-2115").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2020-2686
    #[tokio::test]
    async fn test_2020_2686() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2020-2686").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_2019_0142() {
        let scraper = EuParliamentScraper::default();
        // 2019/0142M(NLE)" prod: 2019-0142
        let voteable = scraper.get_voteable("2019-0142").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_2019_2990() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2019-2990").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2021-2540
    #[tokio::test]
    async fn test_2021_2540() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2021-2540").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_2023_0212() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2023-0212").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());
        let is_main = &voteable.votes.votes[0].is_main;
        assert!(is_main.unwrap());

        let docs = &voteable.proposal.documents;
        dbg!(docs);
    }
    // 2025-0385
    #[tokio::test]
    async fn test_2025_0385() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2025-0385").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());

        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));

        let votes = voteable
            .votes
            .votes
            .iter()
            .find(|vote| vote.is_main.unwrap());
        dbg!(&votes);
    }

    //2026-2617
    #[tokio::test]
    async fn test_2026_2617() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2026-2617").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());
        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    // 2026-2745
    #[tokio::test]
    async fn test_2026_2745() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2026-2745").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());
        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }
    // 2025-0145
    #[tokio::test]
    async fn test_2025_0145() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2025-0145").await.unwrap();
        assert!(!voteable.votes.votes.is_empty());
        assert!(voteable
            .votes
            .votes
            .iter()
            .any(|vote| vote.is_main.unwrap_or_default()));
    }

    #[tokio::test]
    async fn test_all_2026_procedures() {
        let scraper = EuParliamentScraper::default();

        let mut prods_with_votes = 0;
        let mut prods_with_votes_main = 0;
        let mut total_sum_votes = 0;

        // procedure has main votes "2024-0159M"
        // procedure has main votes "2024-0101R"
        for year in [2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026]
            .iter()
            .rev()
        {
            dbg!(year);
            let prods = scraper.list_procedure_ids_by_year(*year).await.unwrap();
            dbg!(prods.len());
            for procedure in prods {
                match scraper.get_voteable(&procedure).await {
                    Err(e) => {
                        println!("failed on procedure: {procedure:?} because of {e:?}");
                        if let ScraperError::TooManyRequests(secs) = e {
                            tokio::time::sleep(secs.unwrap()).await;
                        }
                    }
                    Ok(e) => {
                        if e.votes
                            .votes
                            .iter()
                            .any(|vote| vote.is_main.unwrap_or_default())
                        {
                            println!("procedure has main votes {procedure:?}");
                            prods_with_votes_main += 1;
                        }
                        if e.votes.votes.is_empty() {
                            // println!("procedure has no votes {procedure:?}")
                        } else {
                            prods_with_votes += 1;
                        }
                        total_sum_votes += e.votes.votes.len();
                    }
                }
                // tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            println!("parsed prods with votes (current total): {prods_with_votes}, current has main vote total: {prods_with_votes_main}, total sum of votes (with amendments etc): {total_sum_votes}");
        }
        println!("parsed prods with votes (total): {prods_with_votes}");
    }

    #[tokio::test]
    async fn test_procedure_2026_2557() {
        let mut scraper = EuParliamentScraper::default();

        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };

        let voteable = scraper.get_voteable("2026-2557").await.unwrap();

        assert_eq!(voteable.proposal.gp, "10");
        assert_eq!(
            voteable.proposal.created_at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 1, 12).unwrap()
        );

        assert!(!voteable.has_votes());
        dbg!(voteable);
    }

    // 2026-2585
    #[tokio::test]
    async fn test_procedure_2026_2585() {
        let mut scraper = EuParliamentScraper::default();

        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };

        let voteable = scraper.get_voteable("2026-2585").await.unwrap();

        assert_eq!(voteable.proposal.gp, "2026");
        assert_eq!(
            voteable.proposal.created_at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()
        );
    }
    // 2019/2079
    #[tokio::test]
    async fn test_procedure_2019_2079() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let mut scraper = EuParliamentScraper::default();

        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };

        let voteable = scraper.get_voteable("2019-2079").await.unwrap();

        assert_eq!(voteable.proposal.gp, "2019");
        assert!(voteable.has_votes());
        assert_eq!(
            voteable.proposal.created_at.date_naive(),
            NaiveDate::from_ymd_opt(2019, 6, 27).unwrap()
        );
    }

    // 2019-2552
    #[tokio::test]
    async fn test_procedure_2019_2552() {
        let _ = simple_logger::init_with_level(log::Level::Info);
        let mut scraper = EuParliamentScraper::default();

        scraper.cache_config = CacheConfig {
            cache_dir: "./test-data".into(),
            ignore_current_gp_cache: false,
            redo_secs: u64::MAX,
        };

        let voteable = scraper.get_voteable("2019-2552").await.unwrap();

        assert_eq!(voteable.proposal.gp, "2019");
        assert!(voteable.has_votes());
        assert_eq!(
            voteable.proposal.created_at.date_naive(),
            NaiveDate::from_ymd_opt(2019, 3, 13).unwrap()
        );

        let vote = voteable
            .votes
            .votes
            .iter()
            .find(|vote| vote.is_main == Some(true))
            .unwrap();
        let roll_call_vote = vote.roll_call_vote.as_ref().unwrap();
        assert_eq!(roll_call_vote.favor_count, 435);
        assert_eq!(roll_call_vote.against_count, 156);
        let del_vote = &roll_call_vote.votes[0];
        dbg!(del_vote);
    }
    #[tokio::test]
    async fn test_get_votes_for_2026_2589_2() {
        let scraper = EuParliamentScraper::default();

        let voteable = scraper.get_voteable("2026-2589").await.unwrap();
        assert_eq!(
            voteable.proposal.created_at.date_naive(),
            NaiveDate::from_ymd_opt(2026, 2, 11).unwrap()
        );
    }

    use super::*;
    use regex::Regex;
    use serde::Deserialize;
    use std::{collections::HashSet, io::Read};

    use chrono::NaiveDateTime;
    use std::error::Error;

    #[derive(Debug, Deserialize)]
    pub struct VoteRecord {
        pub id: i64,

        #[serde(deserialize_with = "deserialize_datetime")]
        pub timestamp: NaiveDateTime,

        pub display_title: String,

        #[serde(deserialize_with = "empty_as_none")]
        pub reference: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub description: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub amendment_subject: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub amendment_number: Option<String>,

        #[serde(deserialize_with = "deserialize_bool_loose")]
        pub is_main: bool,

        #[serde(deserialize_with = "empty_as_none")]
        pub procedure_reference: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub procedure_title: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub procedure_type: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub procedure_stage: Option<String>,

        pub count_for: u32,
        pub count_against: u32,
        pub count_abstention: u32,
        pub count_did_not_vote: u32,

        #[serde(deserialize_with = "empty_as_none")]
        pub result: Option<String>,

        #[serde(deserialize_with = "empty_as_none")]
        pub texts_adopted_reference: Option<String>,
    }

    fn empty_as_none<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(if s.trim().is_empty() { None } else { Some(s) })
    }

    fn deserialize_datetime<'de, D>(deserializer: D) -> Result<NaiveDateTime, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let trimmed = s.trim();

        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S") {
            return Ok(dt);
        }
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%d %H:%M:%S%.f") {
            return Ok(dt);
        }
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S") {
            return Ok(dt);
        }
        if let Ok(dt) = NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M:%S%.f") {
            return Ok(dt);
        }
        if trimmed.len() >= 19 {
            let head = &trimmed[..19];
            if let Ok(dt) = NaiveDateTime::parse_from_str(head, "%Y-%m-%d %H:%M:%S") {
                return Ok(dt);
            }
        }

        Err(serde::de::Error::custom(format!(
            "could not parse timestamp: {trimmed:?}"
        )))
    }

    fn deserialize_bool_loose<'de, D>(deserializer: D) -> Result<bool, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        match s.trim().to_ascii_lowercase().as_str() {
            "true" | "1" | "yes" => Ok(true),
            "false" | "0" | "no" | "" => Ok(false),
            other => Err(serde::de::Error::custom(format!(
                "invalid boolean value: {other:?}"
            ))),
        }
    }
    pub fn read_votes_from_str(data: &str) -> Result<Vec<VoteRecord>, Box<dyn Error>> {
        let mut rdr = csv::Reader::from_reader(data.as_bytes());
        let mut records = Vec::new();
        for result in rdr.deserialize() {
            let record: VoteRecord = result?;
            records.push(record);
        }
        Ok(records)
    }

    const HOWTHEYVOTE_VOTES_URL: &str =
        "https://github.com/HowTheyVote/data/releases/latest/download/votes.csv.gz";
    const TERM_9_START_YEAR: u32 = 2019;
    const MIN_HOWTHEYVOTE_COVERAGE: f64 = 0.98;

    fn base_procedure_id(id: &str) -> String {
        match id.split_once('-') {
            Some((year, number)) => {
                let digits: String = number.chars().take_while(|c| c.is_ascii_digit()).collect();
                format!("{year}-{digits}")
            }
            None => id.to_string(),
        }
    }

    async fn parse_howtheyvote(scraper: &EuParliamentScraper) -> Vec<VoteRecord> {
        let csv = fetch_csv(scraper).await;
        read_votes_from_str(&csv).unwrap()
    }

    async fn howtheyvote_procedure_ids(scraper: &EuParliamentScraper) -> HashSet<String> {
        let csv = fetch_csv(scraper).await;

        let reference = Regex::new(r"(\d{4})/(\d{4})[A-Z]?\([A-Z]+\)").unwrap();
        reference
            .captures_iter(&csv)
            .filter_map(|caps| {
                let year = caps.get(1)?.as_str();
                let number = caps.get(2)?.as_str();
                (year.parse::<u32>().ok()? >= TERM_9_START_YEAR).then(|| format!("{year}-{number}"))
            })
            .collect()
    }

    #[tokio::test]
    async fn test_cmp_prods_ids() {
        let scraper = EuParliamentScraper::default();

        let howtheyvote_data = parse_howtheyvote(&scraper).await;

        let reference = Regex::new(r"(\d{4})/(\d{4})[A-Z]?\([A-Z]+\)").unwrap();
        let entries = howtheyvote_data
            .iter()
            .filter(|entry| entry.procedure_reference.is_some() && entry.is_main)
            .collect::<Vec<_>>();
        let prods = entries
            .into_iter()
            .flat_map(|entry| {
                let prod = entry.procedure_reference.as_ref()?;
                Some(
                    reference
                        .captures_iter(&prod)
                        .filter_map(|caps| {
                            let year = caps.get(1)?.as_str();
                            let number = caps.get(2)?.as_str();
                            (year.parse::<u32>().ok()).map(|year| format!("{year}-{number}"))
                        })
                        .collect::<HashSet<_>>(),
                )
            })
            .flatten()
            .collect::<HashSet<_>>();

        let mut set = HashSet::new();
        for year in [
            2014, 2015, 2016, 2017, 2018, 2019, 2020, 2021, 2022, 2023, 2024, 2025, 2026,
        ] {
            dbg!(year);
            let ids = scraper.list_procedure_ids_by_year(year).await.unwrap();
            set.extend(ids);
        }

        let diffs = prods.difference(&set).collect::<Vec<_>>();
        dbg!(&diffs);
        dbg!(diffs.len());
    }

    async fn fetch_csv(scraper: &EuParliamentScraper) -> String {
        let bytes = scraper
            .client()
            .get(HOWTHEYVOTE_VOTES_URL)
            .send()
            .await
            .unwrap()
            .bytes()
            .await
            .unwrap();
        let mut csv = String::new();
        flate2::read::GzDecoder::new(&bytes[..])
            .read_to_string(&mut csv)
            .unwrap();
        csv
    }
}
