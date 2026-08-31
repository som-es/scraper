use chrono::{DateTime, NaiveDateTime, Utc};
use common_scrapes::{Reference, Statement, StatementAuthor};
use serde_json::Value;

use crate::{
    convert_vienna_datetime_to_utc, documents, extract_reference_from_gegenstand_url,
    extract_text_from_html, legis_init::extract_topics, person_url_to_id, AustriaScraper, Scraper,
};

use super::list::StatementListItem;

pub fn parse_vienna_datetime(raw: &str) -> Option<DateTime<Utc>> {
    NaiveDateTime::parse_from_str(raw, "%Y-%m-%dT%H:%M:%S")
        .ok()
        .and_then(convert_vienna_datetime_to_utc)
}

fn extract_statement_references(raw_statement_content: &Value) -> Vec<Reference> {
    raw_statement_content["reference"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|reference| extract_reference_from_gegenstand_url(reference["url"].as_str()?))
        .collect()
}

fn extract_statement_authors(raw_statement_content: &Value) -> Vec<StatementAuthor> {
    raw_statement_content["names"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .flat_map(|name| {
            Some(StatementAuthor {
                name: name["name"].as_str()?.to_string(),
                delegate_id: name["url"].as_str().and_then(person_url_to_id),
                party_code: name["frak_code"].as_str().map(String::from),
                institution: name["ltext"].as_str().map(String::from),
            })
        })
        .collect()
}

pub fn raw_statement_into_statement(
    raw_statement: Value,
    statement_info: StatementListItem,
) -> Option<Statement> {
    let raw_statement_content = &raw_statement["content"];

    let inr = raw_statement_content["inr"].as_u64()?;
    let title = raw_statement_content["title"].as_str()?.to_string();
    let description = raw_statement_content["description"]
        .as_str()
        .unwrap_or_default()
        .to_string();

    let statement_text = raw_statement_content["statement"]
        .as_str()
        .map(extract_text_from_html)
        .map(|statement| statement.trim().to_string())
        .filter(|statement| !statement.is_empty());

    let raw_documents = raw_statement_content["documents"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let documents = documents(&raw_documents, AustriaScraper::BASE_URL).unwrap_or_default();

    let references = extract_statement_references(raw_statement_content);
    let authors = extract_statement_authors(raw_statement_content);

    let stage = raw_statement_content["stages"]
        .as_array()
        .and_then(|stages| stages.first())
        .and_then(|stage| stage["text"].as_str())
        .map(String::from);

    let created_at = parse_vienna_datetime(raw_statement_content["einlangen"].as_str()?)?;
    let updated_at = raw_statement_content["update"]
        .as_str()
        .and_then(parse_vienna_datetime);

    Some(Statement {
        id: inr,
        inr,
        ityp: raw_statement_content["ityp"].as_str()?.to_string(),
        gp: raw_statement_content["gp_code"].as_str()?.to_string(),
        zitation: raw_statement_content["zitation"].as_str()?.to_string(),
        title,
        description,
        statement_text,
        documents,
        authors,
        author_kind: statement_info.author_kind,
        primary_reference: statement_info
            .reference
            .clone()
            .or_else(|| references.last().cloned()),
        references,
        topics: extract_topics(raw_statement_content, "topics").unwrap_or_default(),
        eurovoc_topics: extract_topics(raw_statement_content, "eurovoc").unwrap_or_default(),
        other_keyword_topics: extract_topics(raw_statement_content, "headwords")
            .unwrap_or_default(),
        supports: statement_info.supports,
        stage,
        path: statement_info.path,
        created_at,
        updated_at,
    })
}
