//! Explicitly opt-in checks of the Austrian parliament's live filter API.

use crate::{legis_init::LegislativeInitiatives, AustriaScraper};
use chrono::NaiveDate;

#[tokio::test]
#[ignore = "live parlament.gv.at request; run explicitly with --ignored"]
async fn legislative_filter_returns_paths_for_a_fixed_period() {
    let mut scraper = AustriaScraper::default();
    scraper.scrape_info.gp = vec!["XXVII".into()];
    scraper.scrape_info.date_start = Some(
        NaiveDate::from_ymd_opt(2023, 5, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap(),
    );
    scraper.scrape_info.date_end = Some(
        NaiveDate::from_ymd_opt(2023, 5, 31)
            .unwrap()
            .and_hms_opt(23, 59, 59)
            .unwrap(),
    );

    let raw = scraper.post_legis_payload(Some(&["A"])).await.unwrap();
    let paths = scraper.legis_paths(raw).unwrap();

    assert!(paths.iter().any(|path| path == "/gegenstand/XXVII/A/3373"));
    assert!(paths
        .iter()
        .all(|path| path.starts_with("/gegenstand/XXVII/A/")));
}
