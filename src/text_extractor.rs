use std::ops::{RangeBounds, RangeInclusive};

use common_scrapes::{PlenarSpeech, Speech};
use regex::Regex;

use crate::{
    extract_text_from_html, extract_text_from_html_via_name_attribute,
    extract_text_from_html_via_name_attribute_finding_optimal_solution,
    extract_text_from_html_via_name_attribute_next_section, remove_page_info, remove_plenar_info,
    remove_steno_info,
};

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

#[derive(Default, Debug, Clone)]
enum State {
    /// The extractor is in a state where it has no cached HTML
    #[default]
    NoCache,
    /// The extractor is in a state where it has cached HTML
    Cached(Option<String>),
}

/// Extracts text from an Austrian parliament HTML document containing speeches.
/// An Austrian parliament HTML document is defined as Schrödinger's document.
/// You can't know if it's an entire protocol of speeches or just an isolated speech.
///
/// In the future, this should be replaced by a more generic solution
#[derive(Default, Debug, Clone)]
pub struct TextExtractor {
    pub cached_html: Option<String>,
    state: State,
    pub client: reqwest::Client,
    pub base_url: String,
}

impl TextExtractor {
    pub fn new(base_url: &str) -> Self {
        Self {
            cached_html: None,
            state: State::NoCache,
            client: reqwest::Client::new(),
            base_url: base_url.to_string(),
        }
    }

    pub async fn get_html(
        &mut self,
        url: &str,
        key: Option<&str>,
        attribute_extraction_fn: Option<fn(&str, &str) -> Option<String>>,
    ) -> Option<String> {
        let response = self.client.get(url).send().await.unwrap();
        let mut html = response.text().await.ok()?;
        let start = html.find("<style>").unwrap_or_default();
        let end = html.find("</style>").unwrap_or_default();
        html.replace_range(start..end, "");

        if let Some(key) = key {
            self.state = State::Cached(Some(key.to_string()));

            let text = attribute_extraction_fn
                .unwrap_or(extract_text_from_html_via_name_attribute_finding_optimal_solution)(
                &html,
                url.split('#').nth(1).unwrap(),
            )
            // .inspect(|x| println!("--------------------- {x}\n --------------"))
            .map(|text| remove_steno_info(&text));
            self.cached_html = Some(html);
            return text;
        }
        Some(remove_steno_info(&extract_text_from_html(&html)))
    }
    // https://www.parlament.gv.at/dokument/XXIV/NRSITZ/164/SEITE_0159.html
    pub async fn text_accumulation(
        &mut self,
        gp_url: &str,
        site_range: RangeInclusive<usize>,
        skip_strategy_fn: impl Fn(&str, bool) -> bool,
    ) -> Option<String> {
        let mut text_acc = String::new();
        let mut already_skipped = false;
        for site in site_range.clone() {
            let mut count = 0;
            let mut to_divide = site;
            while to_divide > 0 {
                to_divide /= 10;
                count += 1;
            }

            let add_zeros = (0..(4 - count as i32).max(0))
                .map(|_| '0')
                .collect::<String>();
            let to_fetch = format!("{gp_url}SEITE_{add_zeros}{site}.html");
            dbg!(&to_fetch);
            if let Some(text) = self.extract_text_from_url(&to_fetch, None).await {
                if skip_strategy_fn(&text, already_skipped) {
                    already_skipped = true;
                    continue;
                }
                println!("text_acc: {text}");
                // dbg!(&text);
                text_acc.push_str(&text);
            }
        }
        Some(remove_plenar_info(&remove_page_info(&text_acc)))
    }

    pub async fn extract_text_from_url(
        &mut self,
        speech: &impl HasHTMLUrl,
        search_strategy_fn: Option<fn(&str, &str) -> Option<String>>,
    ) -> Option<String> {
        let url = format!("{}{}", self.base_url, speech.html_url());

        let key = if url.contains('#') {
            speech.html_url().split('#').next()
        } else {
            None
        };

        match &self.state {
            State::NoCache => self.get_html(&url, key, search_strategy_fn).await,
            State::Cached(prev_url) => {
                if prev_url.as_ref().map(|url| url.as_str()) == key {
                    let html = self.cached_html.as_ref().unwrap();
                    let url = url.split('#').nth(1).unwrap();
                    let f = search_strategy_fn.unwrap_or(
                        extract_text_from_html_via_name_attribute_finding_optimal_solution,
                    );
                    return f(html, url).map(|text| remove_steno_info(&text));
                } else {
                    self.cached_html = None;
                    self.state = State::NoCache;
                    self.get_html(&url, key, search_strategy_fn).await
                }
            }
        }
    }
}

pub fn find_optimal_votes_by_name(html: &str, name_attribute: &str) -> Option<String> {
    let probably_contains_votes_yes_indicator =
        Regex::new(r"Mit[\S\s]*Ja[\S\s]*stimmten[\S\s]*die[\S\s]*Abge").unwrap();
    let probably_contains_votes_no_indicator =
        Regex::new(r"Mit[\S\s]*Nein[\S\s]*stimmten[\S\s]*die[\S\s]*Abge").unwrap();

    for parent in 1..3 {
        for skip in 0..6 {
            let variant = extract_text_from_html_via_name_attribute_next_section(
                html,
                name_attribute,
                parent,
                skip,
            )
            .unwrap_or_default();

            let new_lines = variant.chars().filter(|c| *c == '\n').count();
            if new_lines >= 140 {
                continue;
            }

            // checking for one, because if multiple votes are inside, do not process
            if probably_contains_votes_yes_indicator
                .find_iter(&variant)
                .count()
                == 1
                && probably_contains_votes_no_indicator
                    .find_iter(&variant)
                    .count()
                    == 1
            {
                return Some(variant);
            }
        }
    }
    return None;
}

#[cfg(test)]
mod tests {
    use crate::{
        find_optimal_votes_by_name, legis_init::LegislativeInitiatives, plenar::Plenar,
        AustriaScraper, HasHTMLUrl, Scraper, TextExtractor,
    };

    #[tokio::test]
    async fn test_text_extractor_i_1954() {
        let scraper = AustriaScraper::default();

        let init = scraper
            // https://www.parlament.gv.at/gegenstand/XXVII/I/234?json=True
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
        for speech in init.speeches {
            println!("!!!!!!!! speech url {}", speech.html_url());
            let text = extractor.extract_text_from_url(&speech, None).await;
            println!("----------- text: {:?}", text);
        }
    }

    #[tokio::test]
    async fn test_text_extractor_missing_text_not_anymore_since_optimal_check() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let res = extractor
            .extract_text_from_url(
                &"/dokument/XXV/NRSITZ/186/fnameorig_673189.html#RU_173145",
                None,
            )
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let url = "/dokument/XXVII/NRSITZ/135/fnameorig_1463119.html#TEXTOBJ_258489";
        let res = extractor.extract_text_from_url(&url, None).await.unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name_failing_not_anymore() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let url = "/dokument/XXVII/NRSITZ/139/fnameorig_1463126.html#TEXTOBJ_260400";
        let res = extractor
            .extract_text_from_url(&url, Some(find_optimal_votes_by_name))
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name_failing() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let url = "/dokument/XXVII/NRSITZ/213/fnameorig_1632713.html#TEXTOBJ_296821";
        // let res = extractor.extract_text_from_url(&url, None).await.unwrap();
        let res = extractor
            .extract_text_from_url(&url, Some(find_optimal_votes_by_name))
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_wrong_text() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();
        for debate in plenar.debates {
            for speech in debate.speeches {
                if speech.delegate_id != 72959 {
                    continue;
                }
                let text = extractor
                    .extract_text_from_url(&speech, None)
                    .await
                    .unwrap();
                println!("{}", text);
            }
        }
    }

    #[tokio::test]
    async fn test_text_extractor_fail_not_anymore() {
        // /dokument/XXV/NRSITZ/25/fnameorig_359671.html#RU_121870
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let res = extractor
            .extract_text_from_url(
                &"/dokument/XXV/NRSITZ/25/fnameorig_359671.html#RU_121870",
                None,
            )
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_no_steno_info() {
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let res = extractor
            .extract_text_from_url(&"/dokument/XXV/NRSITZ/55/fnameorig_397751.html", None)
            .await
            .unwrap();

        if res.contains("Nationalrat, XXV.GP") {
            panic!("NOPE");
        }
    }

    // https://www.parlament.gv.at/dokument/XXVI/NRSITZ/36/fnameorig_741083.html#Seite_0134.html

    #[tokio::test]
    async fn test_text_extractor_fail_older() {
        // /dokument/XXVI/NRSITZ/36/fnameorig_741083.html#Seite_0134.html
        let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let res = extractor
            .extract_text_from_url(
                &"/dokument/XXVI/NRSITZ/36/fnameorig_741083.html#Seite_0134.html",
                Some(find_optimal_votes_by_name),
                // None
            )
            .await;
        println!("res: {res:?}");
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
            let text = extractor
                .extract_text_from_url(&speech, None)
                .await
                .unwrap();
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
            let text = extractor
                .extract_text_from_url(&speech, None)
                .await
                .unwrap();
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

    #[tokio::test]
    async fn test_text_acc() {
        // https://www.parlament.gv.at/dokument/XXIV/NRSITZ/185/SEITE_0171.html
        let mut text_extractor = TextExtractor::new(AustriaScraper::BASE_URL);

        let text = text_extractor
            .text_accumulation("/dokument/XXIV/NRSITZ/185/", 171..=173, |_, _| false)
            .await;
        println!("text: {text:?}");
    }
}
