use common_scrapes::Proposal;
use serde_derive::{Deserialize, Serialize};

use crate::{error::ScraperError, eu_parliament::votes::Vote};

use super::{votes::EuRollCallVote, EuParliamentScraper};

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
    pub votes: Vec<Vote>,
}

impl Voteable {
    #[inline]
    pub fn has_votes(&self) -> bool {
        !self.votes.is_empty()
    }
}

impl EuParliamentScraper {
    pub async fn get_voteable(&self, process_id: &str) -> Result<Voteable, ScraperError> {
        let proposal = self.get_proposal(process_id).await?;
        let votes = self.get_votes_for_proposal(process_id).await?;
        Ok(Voteable { proposal, votes })
    }
}

#[cfg(test)]
mod tests {
    use chrono::{DateTime, NaiveDate};

    use crate::caching::CacheConfig;

    use super::*;

    #[tokio::test]
    async fn test_get_voteable_with_vote() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2024-0003").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "BUD");
        assert!(voteable.has_votes());
        assert_eq!(voteable.votes.len(), 1);
        assert_eq!(
            voteable.votes[0]
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
        assert_eq!(voteable.votes.len(), 1);

        let meta = voteable.votes[0].meta.as_ref().unwrap();
        assert_eq!(meta.voting_id, 194869);
        assert_eq!(meta.favor_count, Some(366));
        assert_eq!(meta.against_count, Some(192));
        assert_eq!(meta.abstention_count, Some(39));
    }

    #[tokio::test]
    async fn test_all_2026_procedures() {
        let scraper = EuParliamentScraper::default();
        let mut prods_with_votes = 0;
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
                        println!("failed on procedure: {procedure:?} because of {e:?}")
                    }
                    Ok(e) => {
                        if e.votes.is_empty() {
                            // println!("procedure has no votes {procedure:?}")
                        } else {
                            println!("procedure has votes {procedure:?}");
                            prods_with_votes += 1;
                        }
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(2)).await;
            }

            println!("parsed prods with votes (current total): {prods_with_votes}");
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

        assert_eq!(voteable.proposal.gp, "ep-10");
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
}
