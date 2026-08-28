use chrono::{DateTime, Utc};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::eu_parliament::last_path_segment;

#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureFeedEntry {
    pub process_id: String,
    pub title: String,
    pub updated: Option<DateTime<Utc>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Field {
    None,
    Title,
    Id,
    Updated,
}

pub fn parse_procedure_feed_xml(xml: &str) -> Vec<ProcedureFeedEntry> {
    let mut reader = Reader::from_str(xml);

    let mut entries = Vec::new();
    let mut current: Option<ProcedureFeedEntry> = None;
    let mut field = Field::None;

    loop {
        let event = match reader.read_event() {
            Ok(event) => event,
            Err(_) => break,
        };

        match event {
            Event::Start(e) => match e.name().as_ref() {
                b"entry" => {
                    current = Some(ProcedureFeedEntry {
                        process_id: String::new(),
                        title: String::new(),
                        updated: None,
                    })
                }
                b"title" => field = Field::Title,
                b"id" => field = Field::Id,
                b"updated" => field = Field::Updated,
                _ => {}
            },
            Event::Text(t) => {
                let (Some(entry), Ok(text)) = (current.as_mut(), t.unescape()) else {
                    continue;
                };
                match field {
                    Field::Title => entry.title.push_str(text.trim()),
                    Field::Id => entry.process_id = last_path_segment(text.trim()).to_string(),
                    Field::Updated => {
                        entry.updated = DateTime::parse_from_rfc3339(text.trim())
                            .ok()
                            .map(|dt| dt.with_timezone(&Utc));
                    }
                    Field::None => {}
                }
            }
            Event::End(e) => match e.name().as_ref() {
                b"title" | b"id" | b"updated" => field = Field::None,
                b"entry" => {
                    if let Some(entry) = current.take() {
                        entries.push(entry);
                    }
                }
                _ => {}
            },
            Event::Eof => break,
            _ => {}
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version='1.0' encoding='UTF-8'?><feed xmlns="http://www.w3.org/2005/Atom"><id>https://data.europarl.europa.eu/api/v2/procedures/feed</id><updated>2026-07-14T17:33:54.926Z</updated><entry><title type="text" xml:lang="en">Connecting Europe through high-speed rail</title><id>https://data.europarl.europa.eu/eli/dl/proc/2026-2003</id><link href="https://data.europarl.europa.eu/eli/dl/proc/2026-2003" rel="self"/><category term="process-type" scheme="https://data.europarl.europa.eu/def/ep-procedure-types/INI" label="Own-initiative procedure"/><updated>2026-07-14T16:44:02.558Z</updated></entry></feed>"#;

    #[test]
    fn parses_entries() {
        let entries = parse_procedure_feed_xml(SAMPLE);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].process_id, "2026-2003");
        assert_eq!(
            entries[0].title,
            "Connecting Europe through high-speed rail"
        );
        assert_eq!(
            entries[0].updated,
            Some("2026-07-14T16:44:02.558Z".parse().unwrap())
        );

        assert_eq!(entries[0].process_id, "2026-2003");
    }
}
