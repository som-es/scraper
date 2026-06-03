use async_trait::async_trait;

use chrono::Local;
use common_scrapes::{ParliamentaryQAEntity, Proposal, QaReference, QaType};
use serde_json::Value;

use crate::{
    error::ScraperError, extract_reference_from_gegenstand_url, proposals::ProposalScraper,
    LastUpdated,
};

use super::AustriaScraper;

impl LastUpdated for ParliamentaryQAEntity {
    fn last_updated(&self) -> chrono::DateTime<chrono::Utc> {
        self.base_info.updated_at.unwrap_or(Local::now().to_utc())
    }
}

#[async_trait]
pub trait ParliamentaryQAScraper: ProposalScraper {
    async fn post_parliamentary_qa_payload(&self) -> Result<Value, ScraperError> {
        self.post_proposals_payload(Some(&["AB", "J"])).await
    }
    fn raw_value_into_parliamentary_qa(
        raw_value: Value,
        proposal: Proposal,
    ) -> Option<ParliamentaryQAEntity>;

    async fn get_parliamentary_qa(
        &self,
        path: &str,
    ) -> Result<ParliamentaryQAEntity, ScraperError> {
        let raw = self.cache(path).await?;
        let proposal = Self::raw_proposal_into_proposal(raw.clone())
            .ok_or(ScraperError::InvalidProposalInfoError)?;
        Self::raw_value_into_parliamentary_qa(raw, proposal)
            .ok_or(ScraperError::InvalidProposalInfoError)
    }
}

impl ParliamentaryQAScraper for AustriaScraper {
    fn raw_value_into_parliamentary_qa(
        raw_value: Value,
        proposal: Proposal,
    ) -> Option<ParliamentaryQAEntity> {
        let content = raw_value.get("content")?;

        let references = content
            .get("reference")
            .map(|reference| reference.as_array())
            .flatten()
            .map(|references| references.as_slice())
            .unwrap_or(&[]);

        let references = references
            .iter()
            .flat_map(|reference| {
                dbg!(reference);
                let reference = extract_reference_from_gegenstand_url(
                    reference.get("url").unwrap().as_str().unwrap(),
                )
                .unwrap();
                dbg!(&reference);
                let qa_type = extract_qa_type_from_ityp(&reference.ityp)?;
                Some(QaReference { qa_type, reference })
            })
            .collect::<Vec<_>>();

        let qa_type = match extract_qa_type_from_ityp(&proposal.ityp) {
            Some(value) => value,
            None => return None,
        };

        Some(ParliamentaryQAEntity {
            base_info: proposal,
            references,
            qa_type,
        })
    }
}

fn extract_qa_type_from_ityp(ityp: &str) -> Option<QaType> {
    let qa_type = if ityp == "J" {
        QaType::Question
    } else if ityp == "AB" {
        QaType::Answer
    } else {
        return None;
    };
    Some(qa_type)
}

#[cfg(test)]
mod tests {
    use common_scrapes::{QaReference, QaType, Reference};

    use crate::{parliamentary_questions::ParliamentaryQAScraper, AustriaScraper};

    #[tokio::test]
    async fn test_check_for_references_lengths() -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let qa_entity = scraper
            .get_parliamentary_qa("/gegenstand/XXVII/J/14815")
            .await?;
        assert_eq!(qa_entity.receiver_ids, [20445]);
        assert_eq!(qa_entity.issuer_ids, [2344]);
        assert_eq!(qa_entity.qa_type, QaType::Question);
        assert_eq!(
            qa_entity.references[0],
            QaReference {
                qa_type: QaType::Answer,
                reference: Reference {
                    gp: "XXVII".into(),
                    ityp: "AB".into(),
                    inr: 14326
                }
            }
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_get_proposal_from_path_with_issuer_and_receiver(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let scraper = AustriaScraper::default();

        let qa_entity = scraper
            .get_parliamentary_qa("/gegenstand/XXVII/J/14815")
            .await?;
        assert_eq!(qa_entity.receiver_ids, [20445]);
        assert_eq!(qa_entity.issuer_ids, [2344]);
        assert_eq!(qa_entity.qa_type, QaType::Question);

        Ok(())
    }
}
