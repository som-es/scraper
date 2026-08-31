use chrono::NaiveDate;
use common_scrapes::{Reference, StatementAuthorKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::extract_text_from_html;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct StatementListItem {
    pub path: String,
    pub gp: String,
    pub ityp: String,
    pub inr: u64,
    pub zitation: String,
    pub datum: Option<NaiveDate>,
    pub author_name: String,
    pub author_kind: StatementAuthorKind,
    pub reference: Option<Reference>,
    pub supports: u64,
}

impl StatementListItem {
    pub fn new(gp: &str, ityp: &str, inr: u64, author_kind: StatementAuthorKind) -> Self {
        Self {
            path: format!("/gegenstand/{gp}/{ityp}/{inr}"),
            gp: gp.to_string(),
            ityp: ityp.to_string(),
            inr,
            zitation: String::new(),
            datum: None,
            author_name: String::new(),
            author_kind,
            reference: None,
            supports: 0,
        }
    }

    pub fn from_row(row: &Value, indices: &StatementRowIndices) -> Option<Self> {
        let row = row.as_array()?;
        let value = |idx: usize| row.get(idx).cloned().unwrap_or_default();

        let gp = value(indices.gp).as_str()?.to_string();
        let ityp = value(indices.ityp).as_str()?.to_string();
        let inr = row_u64(&value(indices.inr))?;

        let von = value(indices.von).as_str()?.to_string();
        if !von.starts_with("<a") {
            return None;
        }

        let zitation = value(indices.zitation)
            .as_str()
            .unwrap_or_default()
            .to_string();

        let author_kind = StatementAuthorKind::from_code(value(indices.typ).as_str()?)?;

        let datum = value(indices.datum_sort)
            .as_str()
            .and_then(|datum| NaiveDate::parse_from_str(datum, "%Y-%m-%dT%H:%M:%S").ok())
            .or_else(|| {
                value(indices.datum)
                    .as_str()
                    .and_then(|datum| NaiveDate::parse_from_str(datum, "%d.%m.%Y").ok())
            });

        let reference = (|| {
            Some(Reference {
                gp: value(indices.bezug_gp).as_str()?.to_string(),
                ityp: value(indices.bezug_ityp).as_str()?.to_string(),
                inr: row_u64(&value(indices.bezug_inr))?.try_into().ok()?,
            })
        })();

        Some(StatementListItem {
            path: format!("/gegenstand/{gp}/{ityp}/{inr}"),
            author_name: statement_author_name(&von, &zitation),
            supports: row_u64(&value(indices.supports)).unwrap_or_default(),
            gp,
            ityp,
            inr,
            zitation,
            datum,
            author_kind,
            reference,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StatementRowIndices {
    pub gp: usize,
    pub ityp: usize,
    pub inr: usize,
    pub datum: usize,
    pub datum_sort: usize,
    pub von: usize,
    pub bezug_gp: usize,
    pub bezug_inr: usize,
    pub bezug_ityp: usize,
    pub supports: usize,
    pub zitation: usize,
    pub typ: usize,
}

impl Default for StatementRowIndices {
    fn default() -> Self {
        Self {
            gp: 0,
            ityp: 1,
            inr: 2,
            datum: 4,
            datum_sort: 5,
            von: 6,
            bezug_gp: 7,
            bezug_inr: 8,
            bezug_ityp: 9,
            supports: 12,
            zitation: 15,
            typ: 19,
        }
    }
}

impl StatementRowIndices {
    pub fn from_header(header: &Value) -> Self {
        let mut indices = Self::default();

        for column in header.as_array().cloned().unwrap_or_default() {
            let Some(rnr) = column["rnr"].as_u64().map(|rnr| rnr as usize - 1) else {
                continue;
            };
            match column["feld_name"].as_str() {
                Some("GP_CODE") => indices.gp = rnr,
                Some("ITYP") => indices.ityp = rnr,
                Some("INR") => indices.inr = rnr,
                Some("DATUM") => indices.datum = rnr,
                Some("DATUM_SORT") => indices.datum_sort = rnr,
                Some("BEZUG_GP_CODE") => indices.bezug_gp = rnr,
                Some("BEZUG_INR") => indices.bezug_inr = rnr,
                Some("BEZUG_ITYP") => indices.bezug_ityp = rnr,
                Some("TYP") => indices.typ = rnr,
                _ => {}
            }
        }

        indices
    }
}

fn row_u64(value: &Value) -> Option<u64> {
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn statement_author_name(von: &str, zitation: &str) -> String {
    let mut name = extract_text_from_html(von).trim().to_string();
    let citation = format!("({zitation})");
    if name.ends_with(&citation) {
        name.truncate(name.len() - citation.len());
        name = name.trim_end().to_string();
    }
    name
}
