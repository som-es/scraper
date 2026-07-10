use std::collections::HashMap;

use common_scrapes::Party;

use crate::error::ScraperError;

use super::EuParliamentScraper;

/// Best-effort brand colors for the political groups active in recent parliamentary
/// terms. The EU Open Data API has no `color` field anywhere (unlike the Austrian
/// Parlament API, which returns an official `color` per party alongside every vote,
/// see `extract_votes` in `legis_init/extracting.rs`), so these are hardcoded from the
/// color conventions used in the Parliament's own charts and press kits. Treat these as
/// approximate; update when groups are renamed or newly formed.
const GROUP_COLORS: &[(&str, &str)] = &[
    ("PPE", "#3399FF"),
    ("S&D", "#F0001C"),
    ("Renew", "#FFD700"),
    ("Verts/ALE", "#3EB618"),
    ("Greens/EFA", "#3EB618"),
    ("ID", "#2B3856"),
    ("PfE", "#17224D"),
    ("ECR", "#1B9BE0"),
    ("ESN", "#0053A5"),
    ("GUE/NGL", "#B00000"),
    ("The Left", "#B00000"),
    ("NI", "#999999"),
];

fn color_for_code(code: &str) -> String {
    GROUP_COLORS
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, color)| color.to_string())
        .unwrap_or_else(|| "#AAAAAA".to_string())
}

impl EuParliamentScraper {
    /// Derives the currently sitting political groups and their member counts from
    /// `meps/show-current`, rather than from `corporate-bodies` (which lists every
    /// group formed since 1979 without indicating which ones are currently active, and
    /// duplicates recurring groups like GUE/NGL once per term under a different
    /// organization id).
    ///
    /// Unlike AT's `Party`, whose `name` (e.g. "ÖVP") and `code` (e.g. "V") are two
    /// distinct API fields, the EU API only exposes one abbreviation per group (e.g.
    /// "S&D"), so both `name` and `code` are set to it here.
    pub async fn get_parties(&self) -> Result<Vec<Party>, ScraperError> {
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
