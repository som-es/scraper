use somes_scraper::{plenar::Plenar, AustriaScraper};

#[tokio::main]
async fn main() {
    let scraper = AustriaScraper::default();

    let plenar = scraper
        .get_plenar("/gegenstand/XXVII/NRSITZ/211")
        .await
        .unwrap();
    println!("plenar: {plenar:?}");
    dbg!("fin");
}
