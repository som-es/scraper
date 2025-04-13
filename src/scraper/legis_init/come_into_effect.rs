use std::collections::HashMap;

use chrono::NaiveDate;
use common_scrapes::ComeIntoEffectInfo;
use regex::Regex;

fn convert_month_to_number(month: &str) -> Option<usize> {
    let months: HashMap<_, _> = vec![
        ("Jänner", 1),
        ("Januar", 1),
        ("Februar", 2),
        ("März", 3),
        ("April", 4),
        ("Mai", 5),
        ("Juni", 6),
        ("Juli", 7),
        ("August", 8),
        ("September", 9),
        ("Oktober", 10),
        ("November", 11),
        ("Dezember", 12),
        ("January", 1),
        ("February", 2),
        ("March", 3),
        ("April", 4),
        ("May", 5),
        ("June", 6),
        ("July", 7),
        ("August", 8),
        ("September", 9),
        ("October", 10),
        ("November", 11),
        ("December", 12),
    ]
    .into_iter()
    .collect();

    months.get(month).cloned()
}

pub fn extract_german_date_from_str(text: &str) -> Option<NaiveDate> {
    let date_pattern = r"(\d{1,2})\.\s([A-Za-zä]+)\s(\d{4})";
    let date_regex = Regex::new(&date_pattern).unwrap();
    let date_captures = date_regex.captures(text)?;
    let day = date_captures.get(1)?.as_str().trim();
    let month = date_captures.get(2)?.as_str().trim();
    let year = date_captures.get(3)?.as_str().trim();

    let month = convert_month_to_number(month)?;
    NaiveDate::from_ymd_opt(year.parse().ok()?, month as u32, day.parse().ok()?)
}

pub fn extract_come_into_effect(text: &str) -> Option<ComeIntoEffectInfo> {
    let into_effect_regex = r"in\s+Kraft";
    let into_effect_regex = Regex::new(&into_effect_regex).unwrap();
    let Some(come_into_effect_idx) = into_effect_regex.find(text) else {
        return None;
    };

    let come_into_effect = &text[..come_into_effect_idx.start()];

    let date_idx = get_reversed_date_idx(come_into_effect);

    let most_useful_info = &come_into_effect.get(date_idx..)?; /*
    thread 'tokio-runtime-worker' panicked at /home/rusty/Desktop/rust/scraper/src/scraper/legis_init/come_into_effect.rs:64:45:
byte index 12426 is not a char boundary; it is inside '…' (bytes 12424..12427) of `




Entwurf eines Bundesgesetzes über die Anerkennung des
Österreichischen Roten Kreuzes und den Schutz des Zeichens des Roten
Kreuzes (Rotkreuzgesetz - RKG)








Entwurf

Bundesgesetz über die Anerkennung des
Österreichischen Ro`[...]
note: run with `RUST_BACKTRACE=1` environment variable to display a backtrace */

    let date = extract_german_date_from_str(most_useful_info);

    let mut come_into_effect_info = ComeIntoEffectInfo::default();
    come_into_effect_info.law_come_into_effect_date = date;

    if date.is_none() {
        come_into_effect_info.by_publication = true;
    }

    let expire_regex = r"außer\s+Kraft";
    let expire_regex = Regex::new(&expire_regex).unwrap();
    let Some(expire_idx) = expire_regex.find(text) else {
        return Some(come_into_effect_info);
    };
    let expire_str = &text[..expire_idx.start()];

    let date_idx = get_reversed_date_idx(expire_str);

    let most_useful_info = &expire_str[date_idx..];
    let date = extract_german_date_from_str(most_useful_info);
    come_into_effect_info.law_expires_on_date = date;

    Some(come_into_effect_info)
}

fn get_reversed_date_idx(come_into_effect: &str) -> usize {
    let mut date_idx = 0;
    for (idx, char) in come_into_effect.char_indices().rev() {
        if char == '.' {
            date_idx = idx - 1;
            if come_into_effect
                .as_bytes()
                .get(date_idx - 1)
                .map(|x| (*x as char).is_digit(10))
                .unwrap_or_default()
            {
                date_idx -= 1
            }
            break;
        }
    }
    date_idx
}
