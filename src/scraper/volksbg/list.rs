use common_scrapes::{Volksbg, VolksbgEintragungswoche};
use regex::Regex;
use scraper::{ElementRef, Html, Selector};

use super::archive::{live_url, unwrap_archive};
use super::dates::{online_deadline_utc, parse_german_date, parse_german_date_range};
use crate::extract_text_from_html;

/// Volksbegehren pages are `<slug>/start.html` (`start.aspx` in the old CMS layout),/// overview links like `../start.html` are rejected by [`volksbg_id`]
pub const VOLKSBG_LINK_SELECTOR: &str = "a[href*=\"/start.\"]";

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
    let href = unwrap_archive(href).unwrap_or(href);
    let path = href
        .split('#')
        .next()?
        .split('?')
        .next()?
        .trim_end_matches('/');

    let last = path.rsplit('/').next()?;

    let id = if last.starts_with("start.") {
        path[..path.len() - last.len()]
            .trim_end_matches('/')
            .rsplit('/')
            .next()
    } else {
        Some(last)
    }?;

    let is_section_folder = id.chars().all(|character| character.is_numeric());
    let is_volksbg = !id.is_empty()
        && !is_section_folder
        && !id.to_lowercase().ends_with("gesetzgebungsperiode");

    is_volksbg.then(|| id.to_owned())
}

/// Resolves a relative href against the directory of the page it was found on.
/// Archived pages are unwrapped first: `web.archive.org` rewrites the hrefs of
/// its snapshots to archive links, extracted links should point at the live BMI
/// pages the snapshot was taken of, see [`archive`](super::archive)
pub fn absolute_url(href: &str, page_url: &str) -> String {
    let href = live_url(href);
    let page_url = live_url(page_url);
    let (href, page_url) = (href.as_str(), page_url.as_str());

    if href.starts_with("http") {
        return href.to_string();
    }

    let href = href.trim_start_matches('/');
    let directory = page_url
        .rsplit_once('/')
        .map(|(directory, _)| directory)
        .unwrap_or(page_url);

    let base = if href.starts_with("./") || is_root_relative(href, directory) {
        page_url.split('/').take(3).collect::<Vec<_>>().join("/")
    } else {
        directory.to_string()
    };

    let mut joined = format!(
        "{}/{}",
        base.trim_end_matches('/'),
        href.trim_start_matches("./")
    );

    while joined.contains("../") {
        joined = joined.replace("../", "");
    }

    joined
}

/// Links of the old CMS layout are relative to the site root although they don't
/// start with a slash, e.g. `411/Volksbegehren_der_XX_Gesetzgebungsperiode/files/x.pdf`
fn is_root_relative(href: &str, directory: &str) -> bool {
    let first = href.split('/').next().unwrap_or_default();

    !first.is_empty() && directory.split('/').any(|segment| segment == first)
}

/// Detail links of all Volksbegehren of a BMI page. Titles are the link text,
/// except on overview pages that name the Volksbegehren in the heading above.
pub fn parse_volksbg_links(html: &str, page_url: &str) -> Vec<Volksbg> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse(&format!("h2, {VOLKSBG_LINK_SELECTOR}")) else {
        return Vec::new();
    };
    let Ok(column_selector) = Selector::parse("div.text") else {
        return Vec::new();
    };

    let mut heading = String::new();
    let mut seen = std::collections::HashSet::new();

    let elements = match document.select(&column_selector).next() {
        Some(column) => column.select(&selector).collect::<Vec<_>>(),
        None => document.select(&selector).collect::<Vec<_>>(),
    };

    elements
        .into_iter()
        .filter_map(|element| {
            if element.value().name() == "h2" {
                heading = element_text(&element);
                return None;
            }

            let href = element.value().attr("href")?;
            let id = volksbg_id(href)?;
            let link_text = element_text(&element);

            let title = if is_section_label(&link_text) && !heading.is_empty() {
                heading.clone()
            } else {
                link_text
            };

            seen.insert(id.clone()).then_some(Volksbg {
                overview_url: strip_fragment(&absolute_url(href, page_url)),
                title,
                id,
                ..Default::default()
            })
        })
        .collect()
}

fn is_section_label(text: &str) -> bool {
    let text = text.to_lowercase();

    ["einleitungsantrag", "gesamtergebnis", "ergebnis"]
        .iter()
        .any(|label| text.contains(label))
}

fn strip_fragment(url: &str) -> String {
    url.split('#').next().unwrap_or(url).to_string()
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
