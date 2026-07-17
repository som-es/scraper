use std::collections::HashMap;

use async_trait::async_trait;
use common_scrapes::Party;

use crate::{error::ScraperError, parties::Parties};

use super::EuParliamentScraper;

/// Official colors for the EU Parliament's political groups. The EU Open Data API has no
/// `color` field anywhere (unlike the Austrian Parlament API, which returns an official
/// `color` per party alongside every vote, see `extract_votes` in
/// `legis_init/extracting.rs`), so these are taken verbatim from the Parliament's own
/// interactive hemicycle seating tool, whose data feed
/// (`europarl.europa.eu/erpl-public/hemicycle/bru/en.json`) assigns a `color` to every
/// group. The feed lists the current-term groups; both the old and current abbreviation
/// for the two recurring groups are kept so lookups hit regardless of which the API
/// returns (the API uses "PPE", the feed "EPP"). Update when groups are renamed or newly
/// formed.
const GROUP_COLORS: &[(&str, &str)] = &[
    ("PPE", "#003C78"),
    ("EPP", "#003C78"),
    ("S&D", "#C80000"),
    ("Renew", "#FFAA00"),
    ("Verts/ALE", "#32C800"),
    ("Greens/EFA", "#32C800"),
    ("PfE", "#9B20A9"),
    ("ECR", "#0082FF"),
    ("ESN", "#808080"),
    ("GUE/NGL", "#460000"),
    ("The Left", "#460000"),
    ("NI", "#666666"),
];

fn color_for_code(code: &str) -> String {
    GROUP_COLORS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, color)| color.to_string())
        .unwrap_or_else(|| "#AAAAAA".to_string())
}

#[async_trait]
impl Parties for EuParliamentScraper {
    async fn get_parties(&self) -> Result<Vec<Party>, ScraperError> {
        let rows = self.get_all_pages("meps/show-current", "").await?;

        let mut counts: HashMap<String, u32> = HashMap::new();
        for row in &rows {
            let Some(code) = row["api:political-group"].as_str() else {
                continue;
            };
            *counts.entry(code.to_string()).or_default() += 1;
        }

        Ok(counts
            .into_iter()
            .map(|(code, fraction)| Party {
                color: color_for_code(&code),
                name: code.clone(),
                code,
                fraction,
            })
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_get_parties() {
        let scraper = EuParliamentScraper::default();
        let mut parties = scraper.get_parties().await.unwrap();
        parties.sort_by(|a, b| b.fraction.cmp(&a.fraction));

        // group composition/order changes over time, only assert on stable shape
        assert!(!parties.is_empty());
        assert!(parties.iter().any(|p| p.code == "PPE"));
        assert!(parties.iter().map(|p| p.fraction).sum::<u32>() > 500);
    }
}
