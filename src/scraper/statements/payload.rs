use chrono::NaiveDate;
use common_scrapes::Reference;
use serde_json::{json, Value};

use crate::GeneralScrapeInfo;

use super::ityp::StatementItyp;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub struct StatementsPayload {
    pub scrape_info: GeneralScrapeInfo,
    pub ityp: Vec<StatementItyp>,
    pub reference: Option<Reference>,
    pub datum: Vec<NaiveDate>,
}

impl StatementsPayload {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn for_reference(reference: Reference) -> Self {
        Self {
            ityp: StatementItyp::from_reference_ityp(&reference.ityp)
                .map(|ityp| vec![ityp])
                .unwrap_or_default(),
            reference: Some(reference),
            ..Default::default()
        }
    }

    pub fn with_gp(mut self, gp: &str) -> Self {
        self.scrape_info.gp = vec![gp.to_string()];
        self
    }

    pub fn with_ityps(mut self, ityps: Vec<StatementItyp>) -> Self {
        self.ityp = ityps;
        self
    }

    pub fn with_datum(mut self, datum: Vec<NaiveDate>) -> Self {
        self.datum = datum;
        self
    }

    pub fn as_json(&self) -> Value {
        let mut payload = json!({ "VHG": ["1"] });

        if self.reference.is_none() {
            payload["GP_CODE"] = json!(self.scrape_info.gp);
        }

        if !self.ityp.is_empty() {
            payload["ITYP"] = json!(self
                .ityp
                .iter()
                .map(|ityp| ityp.as_str())
                .collect::<Vec<&'static str>>());
        }

        if !self.datum.is_empty() {
            payload["DATUM"] = json!(self
                .datum
                .iter()
                .map(|datum| datum.format("%d.%m.%Y").to_string())
                .collect::<Vec<String>>());
        }

        if let Some(reference) = &self.reference {
            payload["BEZUG_GP_CODE"] = json!([reference.gp]);
            payload["BEZUG_ITYP"] = json!([reference.ityp]);
            payload["BEZUG_INR"] = json!([reference.inr]);
        }

        payload
    }
}
