use somes_scraper::{AustriaScraper, Scraper, SpeechTextInfo, TextExtractor};

#[tokio::main]
async fn main() {
    dbg!("start");
    std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
    let mut extractor = TextExtractor::default();
    let speech_info = SpeechTextInfo {
        site_start: None,
        site_end: None,
        document_url:
            "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/125/fnameorig_1426372.html#RU_253528"
                .into(),
    };
    let text = extractor
        .speech_extraction_with_fit(&speech_info, "Mag. Christian Drobits (S)")
        .await
        .unwrap();

    let len = text.len();
    println!("text: {text}, len: {len}");
    dbg!("fin");
    // assert_eq!(len, 5220);
}
