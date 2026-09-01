use common_scrapes::{Volksbg, VolksbgEintragungswoche};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use super::dates::{online_deadline_utc, parse_german_date, parse_german_date_range};
use crate::extract_text_from_html;

/// The links of the „Aktuelles“ box point into the BMI Volksbegehren pages
pub const VOLKSBG_LINK_SELECTOR: &str = "a[href*=\"gesetzgebungsperiode\"]";

/// Parses the „Aktuelles“ box of `bmi.gv.at/411/start.html`.
/// Returns `None` when no Eintragungszeitraum is announced (e.g. once the
/// results have been established).
pub fn parse_eintragungswoche(html: &str, page_url: &str) -> Option<VolksbgEintragungswoche> {
    let document = Html::parse_document(html);
    let jumbotron = Selector::parse("div.jumbotron").ok()?;
    let volksbg_links = Selector::parse(VOLKSBG_LINK_SELECTOR).ok()?;

    let box_element = document
        .select(&jumbotron)
        .filter(|element| element.select(&volksbg_links).next().is_some())
        .find(|element| {
            let text = element_text(element);
            text.contains("Eintragungszeitraum") && parse_german_date_range(&text).is_some()
        })?;

    let text = element_text(&box_element);
    let (start, end) = parse_german_date_range(&text)?;

    let mut seen = std::collections::HashSet::new();
    let volksbegehren = box_element
        .select(&volksbg_links)
        .filter_map(|link| {
            let href = link.value().attr("href")?;
            let title = element_text(&link);
            Some(Volksbg {
                id: volksbg_id(href)?,
                overview_url: absolute_url(href, page_url),
                title,
                ..Default::default()
            })
        })
        .filter(|volksbg| seen.insert(volksbg.id.clone()))
        .collect();

    Some(VolksbgEintragungswoche {
        start,
        end,
        cut_off_date: parse_after_keyword(&text, "Stichtag"),
        online_deadline_utc: online_deadline_utc(end),
        polling_stations_url: select_url(&box_element, "a[href$=\".xlsx\"]", page_url),
        volksbegehren,
    })
}

/// `volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html#pk_01`
/// => `gratis_verhuetung`
pub fn volksbg_id(href: &str) -> Option<String> {
    let path = href
        .split('#')
        .next()?
        .split('?')
        .next()?
        .trim_end_matches('/');

    let last = path.rsplit('/').next()?;

    if !last.starts_with("start.") {
        return (!last.is_empty()).then(|| last.to_owned());
    }

    path[..path.len() - last.len()]
        .trim_end_matches('/')
        .rsplit('/')
        .next()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// Resolves a relative href against the directory of the page it was found on
pub fn absolute_url(href: &str, page_url: &str) -> String {
    if href.starts_with("http") {
        return href.to_string();
    }

    let directory = page_url
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or(page_url);

    let mut joined = format!(
        "{}/{}",
        directory.trim_end_matches('/'),
        href.trim_start_matches('/')
    );

    while joined.contains("../") {
        joined = joined.replace("../", "");
    }

    joined
}

fn element_text(element: &ElementRef) -> String {
    one_line(&extract_text_from_html(&element.inner_html()))
}

pub fn one_line(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn select_url(element: &ElementRef, selector: &str, page_url: &str) -> Option<String> {
    let selector = Selector::parse(selector).ok()?;
    let url = element.select(&selector).next()?.value().attr("href")?;
    Some(absolute_url(url, page_url))
}

/// `zum Stichtag (11. Mai 2026)` => 2026-05-11
fn parse_after_keyword(text: &str, keyword: &str) -> Option<chrono::NaiveDate> {
    let re = Regex::new(&format!(
        r"(?i){keyword}[^\d]*(\d{{1,2}}\.\s*[\p{{L}}]+\.?\s*\d{{4}})"
    ))
    .ok()?;

    parse_german_date(&re.captures(text)?.get(1)?.as_str().to_string())
}
