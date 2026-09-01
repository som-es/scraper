use async_trait::async_trait;

use chrono::{DateTime, Utc};
use common_scrapes::Statement;
use serde_json::Value;

use crate::{error::ScraperError, LastUpdated};

use super::{AustriaScraper, Scraper};

pub mod ityp;
pub mod list;
pub mod parsing;
pub mod payload;

#[cfg(test)]
mod tests;

pub use ityp::*;
pub use list::*;
pub use payload::*;

pub const DEFAULT_PAGE_SIZE: u64 = 100_000;

#[async_trait]
pub trait StatementsScraper: Scraper {
    const FILTER_PATH: &'static str;
    const PAGED_FILTER_PATH: &'static str;

    async fn post_statements_payload(
        &self,
        statements_payload: StatementsPayload,
    ) -> Result<Value, ScraperError> {
        self.post_payload(
            statements_payload.as_json(),
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    async fn post_statements_payload_paged(
        &self,
        statements_payload: &StatementsPayload,
        page: u64,
        page_size: u64,
    ) -> Result<Value, ScraperError> {
        self.post_payload(
            statements_payload.as_json(),
            format!(
                "{base_url}{filter}&pagesize={page_size}&page={page}",
                base_url = Self::BASE_URL,
                filter = Self::PAGED_FILTER_PATH,
            ),
        )
        .await
    }

    fn raw_statement_into_statement(
        raw_statement: Value,
        statement_info: StatementListItem,
    ) -> Option<Statement> {
        parsing::raw_statement_into_statement(raw_statement, statement_info)
    }

    fn statement_paths(&self, statements: Value) -> Result<Vec<StatementListItem>, ScraperError>;

    fn statement_pages(&self, statements: &Value) -> u64;

    async fn get_statement(
        &self,
        statement_info: StatementListItem,
    ) -> Result<Statement, ScraperError> {
        let raw = self.cache(&statement_info.path).await?;
        Self::raw_statement_into_statement(raw, statement_info)
            .ok_or(ScraperError::InvalidStatementInfoError)
    }

    async fn statement_paths_of_payload(
        &self,
        statements_payload: StatementsPayload,
    ) -> Result<Vec<StatementListItem>, ScraperError> {
        let statements = self.post_statements_payload(statements_payload).await?;
        self.statement_paths(statements)
    }

    async fn paged_statement_paths(
        &self,
        statements_payload: &StatementsPayload,
        page_size: u64,
    ) -> Result<Vec<StatementListItem>, ScraperError> {
        let mut statements = Vec::new();

        let first_page = self
            .post_statements_payload_paged(statements_payload, 1, page_size)
            .await?;
        let pages = self.statement_pages(&first_page);
        statements.extend(self.statement_paths(first_page)?);

        for page in 2..=pages {
            let page = self
                .post_statements_payload_paged(statements_payload, page, page_size)
                .await?;
            statements.extend(self.statement_paths(page)?);
        }

        Ok(statements)
    }

    async fn statements_of_payload(
        &self,
        statements_payload: StatementsPayload,
    ) -> Result<Vec<Statement>, ScraperError> {
        let mut statements = Vec::new();
        for statement_info in self.statement_paths_of_payload(statements_payload).await? {
            match self.get_statement(statement_info).await {
                Ok(statement) => statements.push(statement),
                Err(e) => log::warn!("could not parse statement: {e:?}"),
            }
        }
        Ok(statements)
    }
}

impl StatementsScraper for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/filter/data/142?js=eval&showAll=true&export=true";
    const PAGED_FILTER_PATH: &'static str =
        "/Filter/api/filter/data/142?js=eval&export=true&sortrnr=6&ascDesc=DESC";

    fn statement_pages(&self, statements: &Value) -> u64 {
        statements["pages"].as_u64().unwrap_or(1).max(1)
    }

    fn statement_paths(&self, statements: Value) -> Result<Vec<StatementListItem>, ScraperError> {
        let indices = StatementRowIndices::from_header(&statements["header"]);
        let rows = statements[Self::LIST_ENTRIES]
            .as_array()
            .ok_or(ScraperError::StatementPathsParseError)?;

        Ok(rows
            .iter()
            .flat_map(|row| StatementListItem::from_row(row, &indices))
            .collect())
    }
}

impl LastUpdated for Statement {
    fn last_updated(&self) -> DateTime<Utc> {
        self.updated_at.unwrap_or_else(chrono::Utc::now)
    }
}
