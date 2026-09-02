use chrono::{Datelike, NaiveDate, TimeZone, Utc};
use regex::Regex;

/// `15. Juni 2026`, `22. Jänner 2025`
pub fn parse_german_date(text: &str) -> Option<NaiveDate> {
    let re = Regex::new(r"(\d{1,2})\.\s*([\p{L}]+)\.?,?\s*(\d{4})").ok()?;
    let caps = re.captures(text)?;
    let month = month_of(&caps[2])?;
    chrono::NaiveDate::from_ymd_opt(
        caps[3].parse::<i32>().ok()?,
        month,
        caps[1].parse::<u32>().ok()?,
    )
}

/// `15. Juni bis 22. Juni 2026`, `31. März bis 7. April 2025`
pub fn parse_german_date_range(text: &str) -> Option<(NaiveDate, NaiveDate)> {
    let re = Regex::new(concat!(
        r"(?i)(\d{1,2})\.\s*([\p{L}]+)\.?\s*(?:(\d{4})\s*)?",
        r"bis\s*(\d{1,2})\.\s*([\p{L}]+)\.?\s*(\d{4})"
    ))
    .ok()?;
    let caps = re.captures(text)?;

    let start_month = month_of(&caps[2])?;
    let end_month = month_of(&caps[5])?;
    let end = NaiveDate::from_ymd_opt(
        caps[6].parse::<i32>().ok()?,
        end_month,
        caps[4].parse::<u32>().ok()?,
    )?;

    let start_year = match caps
        .get(3)
        .and_then(|year| year.as_str().parse::<i32>().ok())
    {
        Some(year) => year,
        None if start_month > end_month => end.year() - 1,
        None => end.year(),
    };

    let start = NaiveDate::from_ymd_opt(start_year, start_month, caps[1].parse::<u32>().ok()?)?;

    Some((start, end))
}

/// Online signatures close at 20:00 europe/vienna on the last day
pub fn online_deadline_utc(end: NaiveDate) -> Option<chrono::DateTime<Utc>> {
    let vienna = chrono_tz::Europe::Vienna;
    vienna
        .from_local_datetime(&end.and_hms_opt(20, 0, 0)?)
        .single()
        .map(|deadline| deadline.to_utc())
}

fn month_of(name: &str) -> Option<u32> {
    let name = name
        .trim()
        .trim_end_matches('.')
        .to_lowercase()
        .replace('ä', "ae")
        .replace('ö', "oe")
        .replace('ü', "ue");

    match name.as_str() {
        "jaenner" | "januar" | "jan" => Some(1),
        "februar" | "feb" => Some(2),
        "maerz" | "maerze" | "mar" | "märze" => Some(3),
        "april" | "apr" => Some(4),
        "mai" | "may" => Some(5),
        "juni" | "june" | "jun" => Some(6),
        "juli" | "july" | "jul" => Some(7),
        "august" | "aug" => Some(8),
        "september" | "sept" | "sep" => Some(9),
        "oktober" | "okt" | "october" => Some(10),
        "november" | "nov" => Some(11),
        "dezember" | "dez" | "dec" => Some(12),
        _ => None,
    }
}
