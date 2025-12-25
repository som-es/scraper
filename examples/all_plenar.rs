use somes_scraper::{plenar::Plenar, AustriaScraper, Scraper, TextExtractor};

#[tokio::main]
async fn main() {
    let mut scraper = AustriaScraper::default();

    let mut extractor = TextExtractor::new();

    std::env::set_var("SCRAPER_IGNORE_CACHE_XXVII", "false");
    let mut all_speeches_count = 0;
    'outer: for gp in ["XXVII", "XXVI", "XXV", "XXIV", "XXIII", "XXII", "XXI", "XX"] {
        // 'outer: for gp in ["XXI", "XX"] {
        scraper.scrape_info.gp = vec![gp.to_string()];

        let plenar_infos = scraper.post_plenar_payload().await.unwrap();

        let paths = scraper
            .plenar_paths(plenar_infos)
            .expect("Failed to get paths");
        for path in paths {
            println!("path: {path}");
            // path with no content at 1: /gegenstand/XXVII/NRSITZ/216

            // newer ones do not provide a document

            let plenar_info = scraper.get_plenar(&path).await;
            let Ok(plenar_info) = plenar_info else {
                continue;
            };
            for debate in plenar_info.debates {
                for speech in debate.speeches {
                    if let Some(spoken_text_url) = speech.speech.spoken_text_html_urls.first() {
                        // let text = extractor
                        //     .speech_extraction_with_fit(
                        //         spoken_text_url,
                        //         &speech.speech.delegate_display_name,
                        //     )
                        //     .await
                        //     .unwrap();
                        // if text.len() <= 100 {
                        //     println!("small text: {text}");
                        // }
                        all_speeches_count += 1;
                    }
                }
            }
            // if all_speeches_count % 100 == 0 {
            //     dbg!("all s c: {all_speeches_count}");
            //     break 'outer;
            // }
            // let speeches_count: usize = plenar_info.debates.iter().map(|d| d.speeches.len()).sum();
            // all_speeches_count += speeches_count;
            // println!("plenar_info: {:?}", plenar_info.absences);
        }
    }

    // 20805
    println!("speeches: {all_speeches_count}");
}
