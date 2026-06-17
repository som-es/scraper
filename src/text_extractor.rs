use std::{
    ops::RangeInclusive,
    sync::{Arc, RwLock},
};

use common_scrapes::{Document, PlenarSpeech, Speech, SpeechTextInfo};
use regex::Regex;
use scraper::Html;

use crate::{
    caching::{disk_caching, get_disk_cache},
    http_client, remove_page_info, remove_plenar_info, remove_steno_info,
};

mod text_from_html;
pub use text_from_html::*;
mod fit_speech;
pub use fit_speech::*;

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
        &self
            .spoken_text_html_urls
            .first()
            .expect("Speech has no HTML URL")
            .document_url
    }
}

impl HasHTMLUrl for &PlenarSpeech {
    fn html_url(&self) -> &str {
        &self
            .speech
            .spoken_text_html_urls
            .first()
            .expect("PlenarSpeech has no HTML URL")
            .document_url
    }
}

impl HasHTMLUrl for PlenarSpeech {
    fn html_url(&self) -> &str {
        &self
            .speech
            .spoken_text_html_urls
            .first()
            .expect("PlenarSpeech has no HTML URL")
            .document_url
    }
}

impl HasHTMLUrl for Document {
    fn html_url(&self) -> &str {
        &self.link
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
#[derive(Debug, Clone)]
pub struct TextExtractor {
    pub cached_html: Arc<RwLock<Option<Html>>>,
    state: State,
    pub client: reqwest::Client,
    pub president_regex: Regex,
    pub president_last_regex: Regex,
    pub president_no_one_left: Regex,
    pub only_president: Regex,
    pub steno_info_regex: Regex,
    pub br_steno_info_regex: Regex,
}

unsafe impl Sync for TextExtractor {}
unsafe impl Send for TextExtractor {}

pub const UNREALISTIC_SPEECH_LENGTH: usize = 50000;

impl TextExtractor {
    pub fn new() -> Self {
        let president_regex = Regex::new(r"\nPräsident(?:[^!,])*?:.*[Nn]ächste.*").unwrap();
        let president_last_regex = Regex::new(r"\nPräsident(?:[^!,])*?:.*[lL]etzte.*").unwrap();
        let president_no_one_left =
            Regex::new(r"\nPräsident(?:[^!,])*?:[\s\S]*niemand.*geschlossen").unwrap();
        let only_president = Regex::new(r"\nPräsident(?:[^!,])*?:").unwrap();
        let steno_info_regex =
            Regex::new(r"Nationalrat,[\s\S]*?Stenographisches Protokoll[\s\S]*?/.+?\d+").unwrap();
        let br_steno_info_regex =
            Regex::new(r"Bundesrat[\s\S]*?Stenographisches Protokoll[\s\S]*?/.+?\d+").unwrap();

        Self {
            cached_html: Default::default(),
            state: State::NoCache,
            client: http_client().unwrap(),
            president_regex,
            president_last_regex,
            president_no_one_left,
            only_president,
            steno_info_regex,
            br_steno_info_regex,
        }
    }

    pub fn remove_steno_info(&self, haystack: &str) -> String {
        remove_steno_info(
            haystack,
            &[
                self.steno_info_regex.clone(),
                self.br_steno_info_regex.clone(),
            ],
        )
    }

    pub async fn get_html(
        &mut self,
        url: &str,
        key: Option<&str>,
        cache_key: &str,
        attribute_extraction_fn: Option<fn(&Html, &str) -> Option<String>>,
        only_text: bool,
    ) -> Option<String> {
        let mut html = match get_disk_cache(cache_key, 60 * 60 * 24 * 30).await {
            Some(html) => html,
            None => {
                let response = match self.client.get(url).send().await {
                    Ok(res) => res,
                    Err(e) => {
                        log::error!("url: {url} Error: {e:?}");
                        return None;
                    }
                };
                let html = response.text().await.ok()?;
                if let Err(e) = disk_caching(cache_key, &html).await {
                    log::warn!("could not cache {cache_key}: {e:?}")
                }
                html
            }
        };

        // let mut html = response.text().await.ok()?;
        let start = html.find("<style>").unwrap_or_default();
        let end = html.find("</style>").unwrap_or_default();
        html.replace_range(start..end, "");

        if let Some(key) = key {
            self.state = State::Cached(Some(key.to_string()));

            let url = url.to_string();
            let steno_info_regex = [
                self.steno_info_regex.clone(),
                self.br_steno_info_regex.clone(),
            ];
            let (html, text) = tokio::task::spawn_blocking(move || {
                let html = Html::parse_document(&html);

                let text =
                    attribute_extraction_fn.unwrap_or(
                        extract_text_from_html_via_name_attribute_finding_optimal_solution,
                    )(&html, url.split('#').nth(1).unwrap())
                    // .inspect(|x| println!("--------------------- {x}\n --------------"))
                    .map(|text| remove_steno_info(&text, &steno_info_regex));
                (html, text)
            })
            .await
            .unwrap();

            self.cached_html = Arc::new(RwLock::new(Some(html)));
            return text;
        }
        let text = if only_text {
            let steno_info_regex = [
                self.steno_info_regex.clone(),
                self.br_steno_info_regex.clone(),
            ];
            tokio::task::spawn_blocking(move || {
                remove_steno_info(&extract_text_from_html(&html), &steno_info_regex)
            })
            .await
            .ok()?
        } else {
            html
        };
        Some(text)
    }

    pub async fn fit_speech(&mut self, unfitted_speech: String, name: &str) -> Option<String> {
        let president_regex = self.president_regex.clone();
        let president_last_regex = self.president_last_regex.clone();
        let president_no_one_left = self.president_no_one_left.clone();
        let only_president = self.only_president.clone();
        let name = name.to_string();
        tokio::task::spawn_blocking(move || {
            fit_speech(
                unfitted_speech,
                &name,
                &president_regex,
                &president_last_regex,
                &president_no_one_left,
                &only_president,
            )
        })
        .await
        .ok()
        .flatten()
    }

    pub async fn speech_extraction_with_fit(
        &mut self,
        speech_info: &SpeechTextInfo,
        name: &str,
    ) -> Option<String> {
        let unfitted_speech = self.raw_speech_extraction(speech_info).await?;
        self.fit_speech(unfitted_speech, name).await
    }

    pub async fn raw_speech_extraction(&mut self, speech_info: &SpeechTextInfo) -> Option<String> {
        // TODO: therei s: "https://www.parlament.gv.at/dokument/XXV/NRSITZ/94/fnameorig_525316.html#Seite_0036.html" too
        // this is a candidate that does not work as well: /dokument/XXIV/NRSITZ/150/fnameorig_256172.html#RU_94584

        /*let site_start = speech_info.site_start;
        let mut site_end = speech_info.site_end;
        if site_end.is_none() {
            site_end = site_start;
        }

        match (site_start, site_end) {
            (Some(site_start), Some(site_end)) => {
                let url = &speech_info.document_url;

                let info = if speech_info
                    .document_url
                    .to_ascii_uppercase()
                    .contains("SEITE_")
                    && !speech_info.document_url.contains("#")
                {
                    // https://www.parlament.gv.at/dokument/XXIV/NRSITZ/164/SEITE_0159.html
                    let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '/')?;
                    Some(("SEITE_", last_idx + 1))
                } else if speech_info.document_url.contains("#") {
                    let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '#')?;
                    Some(("Seite_", last_idx + 1))
                } else {
                    None
                };

                if let Some((site_indicator, last_idx)) = info {
                    return self
                        .text_accumulation(
                            &url[..last_idx],
                            site_start as usize..=site_end as usize,
                            |_, _| false,
                            true,
                            site_indicator,
                        )
                        .await;
                }
            }
            _ => {}
        }
        return self
            .extract_text_from_url(&speech_info.document_url, None, true)
            .await;
        todo!();*/
        if speech_info
            .document_url
            .to_ascii_uppercase()
            .contains("SEITE_")
        {
            let url = &speech_info.document_url;
            let (last_idx, _) = url.char_indices().rev().find(|(_, c)| *c == '/')?;

            let Some(site_start) = speech_info.site_start else {
                return None;
            };

            let site_end = match speech_info.site_end {
                Some(end) => end,
                None => site_start,
            };

            self.text_accumulation(
                &url[..last_idx + 1],
                site_start as usize..=site_end as usize,
                |_, _| false,
                true,
                "SEITE_",
            )
            .await
        } else {
            self.extract_text_from_url(&speech_info.document_url, None, true)
                .await
        }
    }

    // https://www.parlament.gv.at/dokument/XXIV/NRSITZ/164/SEITE_0159.html
    pub async fn text_accumulation(
        &mut self,
        gp_url: &str,
        site_range: RangeInclusive<usize>,
        skip_strategy_fn: impl Fn(&str, bool) -> bool,
        only_text: bool,
        site_indicator: &str,
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

            let add_zeros = (0..(4 - count).max(0)).map(|_| '0').collect::<String>();
            let to_fetch = format!("{gp_url}{site_indicator}{add_zeros}{site}.html");
            log::info!("{to_fetch}");
            match self.extract_text_from_url(&to_fetch, None, only_text).await {
                Some(text) => {
                    if skip_strategy_fn(&text, already_skipped) {
                        already_skipped = true;
                        continue;
                    }
                    text_acc.push_str(&text);
                }
                None => break,
            }
        }
        if text_acc.is_empty() {
            return None;
        }
        Some(remove_plenar_info(&remove_page_info(&text_acc)))
    }

    pub async fn extract_text_from_url(
        &mut self,
        speech: &impl HasHTMLUrl,
        search_strategy_fn: Option<fn(&Html, &str) -> Option<String>>,
        only_text: bool,
    ) -> Option<String> {
        let url = speech.html_url();

        let key = if url.contains('#') {
            speech.html_url().split('#').next()
        } else {
            None
        };

        let cache_key = match key {
            Some(key) => key,
            None => speech.html_url(),
        };

        match &self.state {
            State::NoCache => {
                self.get_html(&url, key, cache_key, search_strategy_fn, only_text)
                    .await
            }
            State::Cached(prev_url) => {
                if prev_url.as_ref().map(|url| url.as_str()) == key {
                    let html = self.cached_html.read().unwrap();
                    let url = url.split('#').nth(1).unwrap();
                    let f = search_strategy_fn.unwrap_or(
                        extract_text_from_html_via_name_attribute_finding_optimal_solution,
                    );
                    f(html.as_ref().unwrap(), url).map(|text| self.remove_steno_info(&text))
                } else {
                    self.cached_html = Default::default();
                    self.state = State::NoCache;
                    self.get_html(&url, key, cache_key, search_strategy_fn, only_text)
                        .await
                }
            }
        }
    }
}

fn contains_single_named_votes(yes_indicator: &Regex, no_indicator: &Regex, text: &str) -> bool {
    return yes_indicator.find_iter(&text).count() == 1
        && no_indicator.find_iter(&text).count() == 1;
}

pub fn find_optimal_votes_by_name(document: &Html, name_attribute: &str) -> Option<String> {
    let votes_yes_indicator =
        Regex::new(r"Mit[\S\s]*Ja[\S\s]*stimmten[\S\s]*die[\S\s]*Abge").unwrap();
    let votes_no_indicator =
        Regex::new(r"Mit[\S\s]*Nein[\S\s]*stimmten[\S\s]*die[\S\s]*Abge").unwrap();

    if NAME_ATTRIBUTES
        .iter()
        .any(|attr| name_attribute.starts_with(attr))
    {
        for parent in 1..3 {
            for skip in 0..6 {
                let extract_text_from_html_via_name_attribute_next_section =
                    extract_text_from_html_via_name_attribute_next_section(
                        document,
                        name_attribute,
                        parent,
                        skip,
                    )
                    .unwrap_or_default();
                let variant = extract_text_from_html_via_name_attribute_next_section;

                let new_lines = variant.chars().filter(|c| *c == '\n').count();
                if new_lines >= 140 {
                    continue;
                }

                // checking for one, because if multiple votes are inside, do not process
                if contains_single_named_votes(&votes_yes_indicator, &votes_no_indicator, &variant)
                {
                    return Some(variant);
                }
            }
        }
    } else {
        let text = extract_text_from_html_via_rn_id(document, name_attribute);
        if contains_single_named_votes(&votes_yes_indicator, &votes_no_indicator, &text) {
            return Some(text);
        }
        // it is possible (and it occured) that the parlamentsdirektion adds a wrong sub RN link e.g. https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/41/A_-_17_40_32_00837360.html#121.11
        if let Some(top_name_attribute) = name_attribute.split('.').next() {
            let text = extract_text_from_html_via_rn_id(document, top_name_attribute);
            if contains_single_named_votes(&votes_yes_indicator, &votes_no_indicator, &text) {
                return Some(text);
            }
        }
        return None;
    }
    None
}

#[cfg(test)]
mod tests {

    use crate::{
        find_optimal_votes_by_name, legis_init::LegislativeInitiatives, plenar::Plenar,
        AustriaScraper, HasHTMLUrl, Scraper, TextExtractor,
    };

    // https://www.parlament.gv.at/dokument/XXVII/NRSITZ/233/fnameorig_1627077.html#RU_306028
    #[tokio::test]
    async fn text_another_speech_extraction() {
        let mut extractor = TextExtractor::new();

        let res = extractor
            .extract_text_from_url(
                &"https://www.parlament.gv.at/dokument/XXVII/NRSITZ/233/fnameorig_1627077.html#RU_306028",
                None,
                true,
            )
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn text_another_speech_extraction_with_actual_pages() {
        let scraper = AustriaScraper::default();
        // /gegenstand/XXIII/I/682
        let mut extractor = TextExtractor::new();
        let legis_init = scraper
            .get_legis_init("/gegenstand/XXIII/I/682")
            .await
            .unwrap();
        for speech in legis_init.speeches {
            let speech_info = &speech.spoken_text_html_urls[0];
            println!("speech_info: {speech:?}");
            // let text = extractor.raw_speech_extraction(speech_info).await.unwrap();
            let text = extractor
                .speech_extraction_with_fit(speech_info, &speech.delegate_display_name)
                .await
                .unwrap();
            println!("---------------- text: {text:?}");
        }
    }

    #[tokio::test]
    async fn test_text_extractor_i_1954() {
        let scraper = AustriaScraper::default();

        let init = scraper
            // https://www.parlament.gv.at/gegenstand/XXVII/I/234?json=True
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        let mut extractor = TextExtractor::new();
        for speech in init.speeches {
            println!("!!!!!!!! speech url {}", speech.html_url());
            let text = extractor.extract_text_from_url(&speech, None, true).await;
            println!("----------- text: {}", text.unwrap());
        }
    }

    #[tokio::test]
    async fn test_text_extractor_missing_text_not_anymore_since_optimal_check() {
        let mut extractor = TextExtractor::new();

        let res = extractor
            .extract_text_from_url(
                &"https://www.parlament.gv.at/dokument/XXV/NRSITZ/186/fnameorig_673189.html#RU_173145",
                None,
                true,
            )
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name() {
        let mut extractor = TextExtractor::new();

        let url = "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/135/fnameorig_1463119.html#TEXTOBJ_258489";
        let res = extractor
            .extract_text_from_url(&url, None, true)
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name_failing_not_anymore() {
        let mut extractor = TextExtractor::new();

        let url = "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/139/fnameorig_1463126.html#TEXTOBJ_260400";
        let res = extractor
            .extract_text_from_url(&url, Some(find_optimal_votes_by_name), true)
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_on_votes_by_name_failing() {
        let mut extractor = TextExtractor::new();

        let url = "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/213/fnameorig_1632713.html#TEXTOBJ_296821";
        // let res = extractor.extract_text_from_url(&url, None, true).await.unwrap();
        let res = extractor
            .extract_text_from_url(&url, Some(find_optimal_votes_by_name), true)
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_wrong_text() {
        let mut extractor = TextExtractor::new();

        let scraper = AustriaScraper::default();
        let plenar = scraper
            .get_plenar("/gegenstand/XXVII/NRSITZ/117")
            .await
            .unwrap();
        for debate in plenar.debates {
            for speech in debate.speeches {
                if speech.speech.delegate_id != 72959 {
                    continue;
                }
                let text = extractor
                    .extract_text_from_url(&speech, None, true)
                    .await
                    .unwrap();
                println!("{}", text);
            }
        }
    }

    #[tokio::test]
    async fn test_text_extractor_fail_not_anymore() {
        // /dokument/XXV/NRSITZ/25/fnameorig_359671.html#RU_121870
        let mut extractor = TextExtractor::new();

        let res = extractor
            .extract_text_from_url(
                &"https://www.parlament.gv.at/dokument/XXV/NRSITZ/25/fnameorig_359671.html#RU_121870",
                None,
                true,
            )
            .await
            .unwrap();
        println!("res: {res}");
    }

    #[tokio::test]
    async fn test_text_extractor_no_steno_info() {
        let mut extractor = TextExtractor::new();

        let res = extractor
            .extract_text_from_url(
                &"https://www.parlament.gv.at/dokument/XXV/NRSITZ/55/fnameorig_397751.html",
                None,
                true,
            )
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
        let mut extractor = TextExtractor::new();

        let res = extractor
            .extract_text_from_url(
                &"https://www.parlament.gv.at/dokument/XXVI/NRSITZ/36/fnameorig_741083.html#Seite_0134.html",
                Some(find_optimal_votes_by_name),
                // None
                true,
            )
            .await;
        println!("res: {res:?}");
    }

    /*#[tokio::test]
    async fn test_text_extractor_changing_cache() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/234")
            .await
            .unwrap();

        let mut extractor = TextExtractor::new();

        let mut prev_html = None;
        let mut once = false;
        for speech in init.speeches {
            let text = extractor
                .extract_text_from_url(&speech, None, true)
                .await
                .unwrap();
            let url = format!("{}{}", extractor.base_url, speech.html_url());

            if url.contains("/43/fnameorig_845422.html#RU_221643") {
                assert!(text.contains("den Praxiseinsatz bereits in sehr bewährter Weise"))
            }

            if speech.spoken_text_html_urls[0]
                .document_url
                .contains("BR/BR")
                && once
            {
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
                .extract_text_from_url(&speech, None, true)
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
    }*/

    #[tokio::test]
    async fn test_text_acc() {
        // https://www.parlament.gv.at/dokument/XXIV/NRSITZ/185/SEITE_0171.html
        let mut text_extractor = TextExtractor::new();

        let text = text_extractor
            .text_accumulation(
                "https://www.parlament.gv.at/dokument/XXIV/NRSITZ/185/",
                171..=173,
                |_, _| false,
                true,
                "SEITE_",
            )
            .await;
        println!("text: {text:?}");
    }

    #[tokio::test]
    async fn test_title_back_speech_fit() {
        let speech = "Bundesminister für Land- und Forstwirtschaft, Klima- und Umweltschutz, Regionen und Wasserwirtschaft Mag. Norbert Totschnig, MSc: Vielen Dank, sehr geehrter Herr Präsident! Geschätzte Damen und Herren Nationalrätinnen und Nationalräte! Hohes Haus! Ich möchte mein Statement erstens in die UG 42: Landwirtschaft, Forstwirtschaft und Wasserwirtschaft, und zweitens in die UG 43: Umwelt, Klimawandelanpassung und Kreislaufwirtschaft, unterteilen.

        Zur Landwirtschaft: Mit dem Budget für die Land- und Forstwirtschaft, Wasserwirtschaft und Regionen setzen wir ein klares Zeichen für unsere bäuerlichen Familienbetriebe, für intakte Regionen, für eine nachhaltige Ressourcennutzung. Das ist ein klares Statement für eine ökosoziale Verantwortung, die wir seit vielen Jahren in diesem Land leben. Es stehen rund 3 Milliarden Euro zur Verfügung, genau sind es 2,84 Milliarden Euro für 2025 und 2,88 Milliarden Euro für 2026. Damit sichern wir die Versorgung im Land, damit sichern wir die Lebensqualität im ländlichen Raum.

        Herr Kollege Schmiedlechner, ich weiß nicht, über welches Budget du geredet hast, es waren jedenfalls nicht das Budget 2025 und das Budget 2026. (Abg. Reiter [ÖVP]: Wahrscheinlich hat er es nicht gelesen!) Natürlich sparen wir auch im Bereich der Landwirtschaft ein. Wir sparen in der Verwaltung ein. Das führt dazu, dass wir in Zukunft auch mehr an Effizienz brauchen. Wir müssen KI einsetzen, es werden da und dort auch keine Posten nachbesetzt. Da sparen wir insgesamt 16 Millionen Euro ein. Wenn wir alles zusammenrechnen, inklusive der Mittel für die Förderungen, technische Anpassungen im Bereich der Gemeinsamen Agrarpolitik, in der Landwirtschaft, Forstwirtschaft und beim Wald – vorhin schon angesprochen –, dann reden wir von 125 Millionen Euro in beiden Jahren, bei einem Budget von insgesamt fast 6 Milliarden Euro. Also wir sparen, aber es ist noch genügend Geld da, vor allem in den Bereichen, in denen wir Wert darauf legen, dass die Finanzierung auch weiterhin gesichert ist. (Beifall bei der ÖVP.)";

        let name = "Norbert Totschnig, MSc (ÖVP)";

        let mut text_extractor = TextExtractor::new();
        let fitted_speech = text_extractor.fit_speech(speech.to_string(), name).await;
        let truth_fitted_speech = "Norbert Totschnig, MSc: Vielen Dank, sehr geehrter Herr Präsident! Geschätzte Damen und Herren Nationalrätinnen und Nationalräte! Hohes Haus! Ich möchte mein Statement erstens in die UG 42: Landwirtschaft, Forstwirtschaft und Wasserwirtschaft, und zweitens in die UG 43: Umwelt, Klimawandelanpassung und Kreislaufwirtschaft, unterteilen.\n        Zur Landwirtschaft: Mit dem Budget für die Land- und Forstwirtschaft, Wasserwirtschaft und Regionen setzen wir ein klares Zeichen für unsere bäuerlichen Familienbetriebe, für intakte Regionen, für eine nachhaltige Ressourcennutzung. Das ist ein klares Statement für eine ökosoziale Verantwortung, die wir seit vielen Jahren in diesem Land leben. Es stehen rund 3 Milliarden Euro zur Verfügung, genau sind es 2,84 Milliarden Euro für 2025 und 2,88 Milliarden Euro für 2026. Damit sichern wir die Versorgung im Land, damit sichern wir die Lebensqualität im ländlichen Raum.\n        Herr Kollege Schmiedlechner, ich weiß nicht, über welches Budget du geredet hast, es waren jedenfalls nicht das Budget 2025 und das Budget 2026. (Abg. Reiter [ÖVP]: Wahrscheinlich hat er es nicht gelesen!) Natürlich sparen wir auch im Bereich der Landwirtschaft ein. Wir sparen in der Verwaltung ein. Das führt dazu, dass wir in Zukunft auch mehr an Effizienz brauchen. Wir müssen KI einsetzen, es werden da und dort auch keine Posten nachbesetzt. Da sparen wir insgesamt 16 Millionen Euro ein. Wenn wir alles zusammenrechnen, inklusive der Mittel für die Förderungen, technische Anpassungen im Bereich der Gemeinsamen Agrarpolitik, in der Landwirtschaft, Forstwirtschaft und beim Wald – vorhin schon angesprochen –, dann reden wir von 125 Millionen Euro in beiden Jahren, bei einem Budget von insgesamt fast 6 Milliarden Euro. Also wir sparen, aber es ist noch genügend Geld da, vor allem in den Bereichen, in denen wir Wert darauf legen, dass die Finanzierung auch weiterhin gesichert ist. (Beifall bei der ÖVP.)";

        assert_eq!(truth_fitted_speech, fitted_speech.unwrap());
    }
}
