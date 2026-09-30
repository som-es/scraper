//! Manual diagnostic scan; intentionally not a regression test.
//! Usage: cargo run --example inspect_legislative_initiatives -- XXVII

use std::{collections::BTreeSet, error::Error};

use somes_scraper::{
    legis_init::{LegislativeInitiatives, LEGIS_IS_PLENARY_VOTEABLE_ON_DOKTYPES},
    AcceptedState, AustriaScraper, FileType,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let gp = std::env::args()
        .nth(1)
        .ok_or("provide a legislative period, e.g. XXVII")?;
    let mut scraper = AustriaScraper::default();
    scraper.scrape_info.gp = vec![gp];
    let raw = scraper
        .post_legis_payload(Some(&LEGIS_IS_PLENARY_VOTEABLE_ON_DOKTYPES))
        .await?;
    let paths = scraper.legis_paths(raw)?;
    let mut document_types = BTreeSet::new();
    let mut failures = 0;

    for path in &paths {
        let init = match scraper.get_legis_init(path).await {
            Ok(init) => init,
            Err(error) => {
                failures += 1;
                eprintln!("{path}: {error:?}");
                continue;
            }
        };
        if init
            .documents
            .iter()
            .any(|doc| doc.title.contains("Gesetz") && doc.ftype == FileType::HTML)
        {
            document_types.insert(init.doktyp.clone());
        }
        println!(
            "{}: type={}, status={:?}, law={}, accepted={:?}, simple_majority={}, named_vote={}, predeclined={}",
            init.id, init.doktyp, init.status, init.is_law, init.accepted,
            init.requires_simple_majority, init.hidden_votes_info.voted_by_name,
            init.accepted == Some(AcceptedState::PreDeclined),
        );
        if let Some(names) = init
            .hidden_votes_info
            .voted_by_name_data
            .and_then(|vote| vote.delegates_pro_contra)
        {
            println!(
                "  named votes: {} for, {} against",
                names.pro.len(),
                names.contra.len()
            );
            for delegate in names.pro.iter().chain(&names.contra) {
                if delegate.name.contains("Home")
                    || delegate.name.contains("Gesamtes ProtokollVorherig")
                {
                    eprintln!(
                        "  possible navigation text in delegate name: {:?}",
                        delegate.name
                    );
                }
            }
        }
    }
    println!("HTML law document types: {document_types:?}");
    println!("{} paths, {failures} parsing/request failures", paths.len());
    if failures > 0 {
        return Err("some legislative initiatives could not be scraped".into());
    }
    Ok(())
}
