use common_scrapes::Proposal;
use serde_derive::{Deserialize, Serialize};

use crate::error::ScraperError;

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
    /// Empty when the procedure exists but has no recorded roll-call vote — either it
    /// hasn't been voted on yet, or it was decided by hand vote rather than a roll
    /// call (see `EuParliamentScraper::get_votes_for_proposal`). More than one entry
    /// when a procedure went through multiple roll-call votes (e.g. separate first-
    /// and second-reading votes).
    pub votes: Vec<EuRollCallVote>,
}

impl Voteable {
    #[inline]
    pub fn has_votes(&self) -> bool {
        !self.votes.is_empty()
    }
}

impl EuParliamentScraper {
    /// Fetches a procedure and whatever roll-call vote(s) decided it as a single
    /// [`Voteable`] — "does this proposal exist, and has it already been voted on" is
    /// one call instead of two whose results the caller must correlate themselves.
    pub async fn get_voteable(&self, process_id: &str) -> Result<Voteable, ScraperError> {
        let proposal = self.get_proposal(process_id).await?;
        let votes = self.get_votes_for_proposal(process_id).await?;
        Ok(Voteable { proposal, votes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_voteable_with_vote() {
        let scraper = EuParliamentScraper::default();
        let voteable = scraper.get_voteable("2024-0003").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "BUD");
        assert!(voteable.has_votes());
        assert_eq!(voteable.votes.len(), 1);
        assert_eq!(voteable.votes[0].favor_count, 547);
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
        assert_eq!(voteable.votes.len(), 1);
        assert_eq!(voteable.votes[0].voting_id, "168997");
        assert_eq!(voteable.votes[0].favor_count, 511);
    }

    #[tokio::test]
    async fn test_get_voteable_committee_report_reference_format() {
        let scraper = EuParliamentScraper::default();

        // 2025-2211 ("Feasibility of a 28th tax regime and its potential to support EU
        // competitiveness", an INI own-initiative report) exercises the *other* label
        // format `get_votes_for_proposal` has to handle: its vote's `<label>` is
        // "Report: Ľudovít Ódor (A10-0167/2026) (Majority of votes cast required)" —
        // matched via `report_references`, not the `{year}/{inr}({ityp})` procedure
        // reference (which would be "2025/2211(INI)" here and does not appear in the
        // label at all). Voted 2026-07-09, 366 favor / 192 against / 39 abstention.
        let voteable = scraper.get_voteable("2025-2211").await.unwrap();

        assert_eq!(voteable.proposal.ityp, "INI");
        assert!(voteable.has_votes());
        assert_eq!(voteable.votes.len(), 1);
        assert_eq!(voteable.votes[0].voting_id, "194869");
        assert_eq!(voteable.votes[0].favor_count, 366);
        assert_eq!(voteable.votes[0].against_count, 192);
        assert_eq!(voteable.votes[0].abstention_count, 39);
    }
}
