use common_scrapes::{PlenarSpeech, Speech};

use crate::{extract_text_from_html, extract_text_from_html_via_name_attribute};

pub trait HasHTMLUrl {
    fn html_url(&self) -> &str;
}

impl HasHTMLUrl for String {
    fn html_url(&self) -> &str {
        self
    }
}

impl HasHTMLUrl for &str {
    fn html_url(&self) -> &str {
        self
    }
}

impl HasHTMLUrl for Speech {
    fn html_url(&self) -> &str {
        self.spoken_text_html_urls
            .first()
            .expect("Speech has no HTML URL")
    }
}

impl HasHTMLUrl for &PlenarSpeech {
    fn html_url(&self) -> &str {
        self.spoken_text_html_urls
            .first()
            .expect("PlenarSpeech has no HTML URL")
    }
}

impl HasHTMLUrl for PlenarSpeech {
    fn html_url(&self) -> &str {
        self.spoken_text_html_urls
            .first()
            .expect("PlenarSpeech has no HTML URL")
    }
}


enum State {
    /// The extractor is in a state where it has no cached HTML
    NoCache,
    /// The extractor is in a state where it has cached HTML
    Cached(Option<String>),
}

/// Extracts text from an Austrian parliament HTML document containing speeches.
/// An Austrian parliament HTML document is defined as Schrödinger's document.
/// You can't know if it's an entire protocol of speeches or just an isolated speech.
///
/// In the future, this should be replaced by a more generic solution
pub struct TextExtractor {
    pub cached_html: Option<String>,
    state: State,
    pub base_url: String,
}

impl TextExtractor {
    pub fn new(base_url: &str) -> Self {
        Self {
            cached_html: None,
            state: State::NoCache,
            base_url: base_url.to_string(),
        }
    }

    pub async fn get_html(&mut self, url: &str, key: Option<&str>) -> String {
        let response = reqwest::get(url).await.unwrap();
        let mut html = response.text().await.unwrap();
        let start = html.find("<style>").unwrap_or_default();
        let end = html.find("</style>").unwrap_or_default();
        html.replace_range(start..end, "");

        if let Some(key) = key {
            self.state = State::Cached(Some(key.to_string()));
            
            let text = extract_text_from_html_via_name_attribute(
                &html,
                url.split('#').nth(1).unwrap(),
            );
            self.cached_html = Some(html);
            return text;
        }
        extract_text_from_html(&html)
    }

    pub async fn extract_text_from_url(&mut self, speech: &impl HasHTMLUrl) -> String {
        let url = format!("{}{}", self.base_url, speech.html_url());

        let key = if url.contains('#') {
            speech.html_url().split('#').next()
        } else {
            None
        };

        match &self.state {
            State::NoCache => self.get_html(&url, key).await,
            State::Cached(prev_url) => {
                if prev_url.as_ref().map(|url| url.as_str()) == key {
                    return extract_text_from_html_via_name_attribute(
                        self.cached_html.as_ref().unwrap(),
                        url.split('#').nth(1).unwrap(),
                    );
                } else {
                    self.cached_html = None;
                    self.state = State::NoCache;
                    self.get_html(&url, key).await
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        legis_init::LegislativeInitiatives, AustriaScraper, HasHTMLUrl, Scraper, TextExtractor, extract_text_from_html_via_name_attribute, plenar::Plenar,
    };

    #[tokio::test]
    async fn test_text_extractor() {
        let scraper = AustriaScraper::default();

        let init = scraper
            // https://www.parlament.gv.at/gegenstand/XXVII/I/234?json=True
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for speech in init.speeches {
            let text = extractor.extract_text_from_url(&speech).await;
            println!("{}", text);
        }
    }

    #[tokio::test]
    async fn test_text_extractor_missing_text() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let res = extractor.extract_text_from_url(&"/dokument/XXV/NRSITZ/186/fnameorig_673189.html#RU_173145").await;
        println!("res: {res:?}");
    }

    #[tokio::test]
    async fn test_text_extractor_wrong_text() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper.get_plenar("/gegenstand/XXVII/NRSITZ/117").await.unwrap();
        for debate in plenar.debates {
            for speech in debate.speeches {
                if speech.delegate_id != 72959 {
                    continue;
                }
                let text = extractor.extract_text_from_url(&speech).await;
                println!("{}", text);
            }
        }

    }

    #[tokio::test]
    async fn test_text_extractor_changing_cache() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/234")
            .await
            .unwrap();

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let mut prev_html = None;
        let mut once = false;
        for speech in init.speeches {
            let text = extractor.extract_text_from_url(&speech).await;
            let url = format!("{}{}", extractor.base_url, speech.html_url());

            if url.contains("/43/fnameorig_845422.html#RU_221643") {
                assert!(text.contains("den Praxiseinsatz bereits in sehr bewährter Weise"))
            }

            if speech.spoken_text_html_urls[0].contains("BR/BR") && once {
                assert_ne!(prev_html, extractor.cached_html);
                once = false;
            }

            if url == "https://www.parlament.gv.at/dokument/BR/BRSITZ/911/fnameorig_852569.html#R_223638_9" {
                assert!(text.contains("ochschulwesen insgesamt nicht guttun, wie auch viele, viele kritische"))
            }

            if let Some(prev_html) = &prev_html {
                assert!(prev_html.len() / 2 > text.len())
            }

            prev_html = extractor.cached_html.clone();
        }

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        for speech in init.speeches {
            let text = extractor.extract_text_from_url(&speech).await;
            let url = format!("{}{}", extractor.base_url, speech.html_url());

            if url == "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/205/A_-_14_40_19_00292508.html" {
                assert!(text.contains("Wenn man sonst nichts hat, doktert man ein bisschen"))
            }

            if url
                == "https://www.parlament.gv.at/dokument/BR/BRSITZ/952/A_-_12_09_27_00293857.html"
            {
                assert!(text
                    .contains("ch bin Polizist und fange bei der Asfinag an, dort bin ich dann"))
            }

            if url
                == "https://www.parlament.gv.at/dokument/BR/BRSITZ/952/A_-_12_27_36_00293862.html"
            {
                assert!(text.contains("hat und sich hinterher gedacht hat: Das"))
            }

            if url
                == "https://www.parlament.gv.at/dokument/BR/BRSITZ/952/A_-_12_20_14_00293852.html"
            {
                assert!(text.contains("b das die Letzte Generation ist oder das Letzte, und stellt"))
            }

            assert_eq!(extractor.cached_html, None);
        }
    }
}
