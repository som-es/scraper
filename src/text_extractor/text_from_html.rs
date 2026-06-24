use std::ops::Deref;

use regex::Match;
use scraper::{ElementRef, Html, Selector};

use crate::{text_utils::remove_paran_contents, TextExtractor};

pub fn extract_text_from_html(html: &str) -> String {
    let (_, text) = remove_paran_contents(html, '<', '>');
    html_escape::decode_html_entities(&text).to_string()
}

/// used if the id does not directly point to the text, but to an element that is completely outside the text
pub fn extract_text_from_html_till_next_name_attribute(html: &str, name_attribute: &str) -> String {
    let document = Html::parse_document(html);

    let selector = Selector::parse(&format!("[name=\"{name_attribute}\"]")).unwrap();

    let el = ElementRef::wrap(
        document
            .select(&selector)
            .next()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    )
    .unwrap();
    let start_at_tree_traversal = el.id();

    let nodes = el
        .tree()
        .nodes()
        .skip_while(|node| node.id() != start_at_tree_traversal);

    let mut count = 0;
    // for node in el.traverse() {

    let mut spoken_text = String::new();

    for node in nodes {
        match node.value() {
            ::scraper::Node::Element(el) => {
                let thing = el.attrs().find(|x| match x {
                    (name, _) if *name == "name" => true,
                    _ => false,
                });

                // println!("thing: {thing:?}");
                if let Some(thing) = thing {
                    if !thing.1.starts_with("RU") {
                        continue;
                    }
                    if count == 1 {
                        break;
                    }
                    count += 1;
                } else {
                    // get inner text
                    // let text = node.children().flat_map(|node| node.value().as_text().map(|text| text.deref())).collect::<String>();
                }
            }
            ::scraper::Node::Text(el) => {
                // let start = node.value().as_text().map(|text| text.deref());
                spoken_text.push_str(el.deref());
            }
            _ => continue,
        };

        /*match node {
            ego_tree::iter::Edge::Open(node) => {

            },
            _ => continue
        }*/
    }
    spoken_text
}

pub fn extract_text_from_html_via_rn_id(document: &Html, name_attribute: &str) -> String {
    let selector = Selector::parse(&format!("[id=\"{}\"]", name_attribute)).unwrap();

    document
        .select(&selector)
        .flat_map(|node| {
            let mut html = String::new();
            let mut current_sibling = *node;
            while let Some(sibling) = current_sibling.next_sibling() {
                let Some(el) = ElementRef::wrap(sibling) else {
                    break;
                };
                if el.attr("class") == Some("randnummer") {
                    break;
                }
                html.push_str(&format!("{}\n", el.html()));
                current_sibling = sibling;
            }

            let text = extract_text_from_html(&html);
            Some(text)
        })
        .collect::<String>()
}

pub fn extract_text_from_html_via_name_attribute_parent_parent(
    document: &Html,
    name_attribute: &str,
) -> String {
    let selector = Selector::parse(&format!("p > a[name=\"{}\"]", name_attribute)).unwrap();

    document
        .select(&selector)
        .flat_map(|node| {
            let html = ElementRef::wrap(node.parent()?.parent()?)?.html();
            let text = extract_text_from_html(&html);
            // let (_, text) = remove_paran_contents(&html, '<', '>');
            // let text = text.replace("&nbsp;", " ");
            Some(text)
        })
        .collect::<String>()
}

pub fn extract_text_from_html_via_name_attribute(document: &Html, name_attribute: &str) -> String {
    let selector = Selector::parse(&format!("[name=\"{}\"]", name_attribute)).unwrap();

    document
        .select(&selector)
        .flat_map(|node| {
            // let html = ElementRef::wrap(node.last_child()?)?.html();
            let html = node.html();
            let text = extract_text_from_html(&html);
            // let (_, text) = remove_paran_contents(&html, '<', '>');
            // let text = text.replace("&nbsp;", " ");
            Some(text)
        })
        .collect::<String>()
}

pub const NAME_ATTRIBUTES: [&str; 4] = ["RU_", "R_", "T_", "TEXTOBJ_"];

fn compress_matches<'h>(matches: Vec<Match<'h>>) -> Vec<Match<'h>> {
    let mut end = None;
    matches
        .into_iter()
        .filter(|matched| {
            let keep = if Some(matched.start() - 1) == end {
                false
            } else {
                true
            };
            end = Some(matched.end());
            keep
        })
        .collect()
}

pub fn extract_text_from_html_via_name_attribute_finding_optimal_solution(
    document: &Html,
    name_attribute: &str,
    text_extrator: TextExtractor,
) -> Option<String> {
    if NAME_ATTRIBUTES
        .iter()
        .any(|attr| name_attribute.starts_with(attr))
    {
        let mut text = String::new();
        let mut tracked_matches_count = 0;
        for section in 0..20 {
            let variant = extract_text_from_html_via_name_attribute_next_section(
                document,
                name_attribute,
                2,
                section,
            )
            .unwrap_or_default();
            let matches = compress_matches(
                text_extrator
                    .fit_speech_regexes
                    .time_marker_re
                    .find_iter(&variant)
                    .collect::<Vec<_>>(),
            );
            if matches.len() > 2 {
                if text.is_empty() {
                    return Some(variant[..matches[1].end()].to_owned());
                } else {
                    return None;
                }
            }
            if let Some(_matched) = matches.get(0) {
                text.push_str(&variant);
                tracked_matches_count += 1;
            } else {
                text.push_str(&variant);
            }

            if tracked_matches_count == 2 || matches.len() == 2 {
                break;
            }
        }

        Some(text)
    } else {
        // resolve via Randnummern RN
        Some(extract_text_from_html_via_rn_id(document, name_attribute))
    }
}

pub fn extract_text_from_html_via_name_attribute_next_section(
    document: &Html,
    name_attribute: &str,
    parents: usize,
    sections: usize,
) -> Option<String> {
    let selector = Selector::parse(&format!("[name=\"{}\"]", name_attribute)).unwrap();

    let mut node = document.select(&selector).next()?.parent()?;
    for _ in 0..parents - 1 {
        node = node.parent()?;
    }
    for _ in 0..sections * 4 {
        node = node.next_sibling()?;
    }
    let html = ElementRef::wrap(node)?.html();
    // let (_, text) = remove_paran_contents(&html, '<', '>');
    // Some(text.replace("&nbsp;", " "))
    Some(extract_text_from_html(&html))

    // Some(ElementRef::wrap(node)?.text().collect::<String>())

    // document
    //     .select(&selector)
    //     .flat_map(|node| {
    //         println!("node: {}", ElementRef::wrap(node.parent()?.clone()).unwrap().html());
    //         let mut node = node.parent()?.parent()?;
    //         for _ in 0..sections * 4 {
    //             node = node.next_sibling()?;
    //         }
    //         Some(ElementRef::wrap(node)?.text().collect::<String>())
    //         // Some(

    //         //     ElementRef::wrap(node.parent()?.parent()?)?
    //         //         .text()
    //         //         .collect::<String>(),
    //         // )
    //     })
    //     .collect::<String>()
}
