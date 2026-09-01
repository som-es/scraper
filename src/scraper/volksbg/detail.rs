use common_scrapes::{Document, FileType, Volksbg};
use regex::Regex;
use scraper::{Html, Selector};

use super::dates::parse_german_date;
use super::list::{absolute_url, one_line};
use crate::extract_text_from_html;

const DOCUMENT_SELECTOR: &str = "a[href$=\".pdf\"], a[href$=\".csv\"], a[href$=\".xlsx\"]";

const WORTLAUT_MARKER: &str = "Das Volksbegehren hat folgenden Wortlaut";

/// After the Wortlaut follow the Begründung links, the Bevollmächtigte and the results
const WORTLAUT_END_MARKERS: [&str; 3] = ["<h2", "<span class=\"bmifile\"", "</main>"];

/// Fills in everything the „Aktuelles“ box doesn't carry: Wortlaut, documents
/// and the dates of the BMI ruling (Beginn / Ende des Eintragungszeitraumes, Stichtag)
pub fn merge_volksbg_detail(volksbg: &mut Volksbg, html: &str, page_url: &str) {
    let text = one_line(&extract_text_from_html(html));

    if volksbg.description.is_none() {
        volksbg.description = parse_wortlaut(html);
    }

    if volksbg.title.is_empty() {
        volksbg.title = first_text_of(html, "h1");
    }

    volksbg.ruling_date = parse_after_keyword(&text, "hat am").or(volksbg.ruling_date);
    volksbg.cut_off_date = parse_after_keyword(&text, "Stichtag:").or(volksbg.cut_off_date);

    let documents = parse_documents(html, page_url);
    if !documents.is_empty() {
        volksbg.documents = documents;
    }
}

/// The Wortlaut are the elements between its heading and the next section
pub fn parse_wortlaut(html: &str) -> Option<String> {
    let start = html.find(WORTLAUT_MARKER)?;
    let after_marker = &html[start..];
    let end = WORTLAUT_END_MARKERS
        .iter()
        .filter_map(|marker| after_marker.find(marker))
        .min()
        .unwrap_or(after_marker.len());

    let wortlaut = one_line(&extract_text_from_html(&after_marker[..end]))
        .trim_start_matches(WORTLAUT_MARKER)
        .trim_start_matches(':')
        .trim()
        .to_string();

    (!wortlaut.is_empty()).then_some(wortlaut)
}

/// Documents of the article column, navigation links excluded
fn parse_documents(html: &str, page_url: &str) -> Vec<Document> {
    let document = Html::parse_document(html);
    let Ok(selector) = Selector::parse(DOCUMENT_SELECTOR) else {
        return Vec::new();
    };
    let Ok(column_selector) = Selector::parse("div.text") else {
        return Vec::new();
    };

    let raw_links = match document.select(&column_selector).next() {
        Some(column) => column.select(&selector).collect::<Vec<_>>(),
        None => document.select(&selector).collect::<Vec<_>>(),
    };

    raw_links
        .into_iter()
        .filter_map(|link| Some((link.value().attr("href")?.to_string(), link.inner_html())))
        .filter(|(href, _)| !href.starts_with('#'))
        .map(|(href, title)| Document {
            title: one_line(&extract_text_from_html(&title)),
            ftype: file_type(&href),
            link: absolute_url(&href, page_url),
        })
        .collect()
}

fn file_type(href: &str) -> FileType {
    match href
        .split(['?', '#'])
        .next()
        .and_then(|href| href.rsplit('.').next())
    {
        Some("pdf") => FileType::OriginalPDF,
        Some("html") | Some("htm") => FileType::HTML,
        _ => FileType::Unknown,
    }
}

fn first_text_of(html: &str, selector: &str) -> String {
    let document = Html::parse_document(html);
    Selector::parse(selector)
        .ok()
        .and_then(|selector| {
            document
                .select(&selector)
                .next()
                .map(|element| one_line(&extract_text_from_html(&element.inner_html())))
        })
        .unwrap_or_default()
}

/// `Beginn des Eintragungszeitraumes: 15. Juni 2026` => 2026-06-15
pub fn parse_after_keyword(text: &str, keyword: &str) -> Option<chrono::NaiveDate> {
    let re = Regex::new(&format!(
        r"(?i){}[:\s]*(\d{{1,2}}\.\s*[\p{{L}}]+\.?\s*\d{{4}})",
        regex::escape(keyword)
    ))
    .ok()?;

    parse_german_date(re.captures(text)?.get(1)?.as_str())
}
