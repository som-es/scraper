use std::collections::HashSet;

use common_scrapes::{Document, FileType};
use regex::Regex;

use crate::{filter::filter_name_extraction, plenar::Absences, unique_titles, TextExtractor};

pub(super) async fn extract_absences(documents: &[Document]) -> Option<Absences> {
    for document in documents {
        if document.ftype == FileType::HTML
            && document.title.to_lowercase().contains("stenographisches")
        {
            let Some(extracted_absences) = extract_absences_from_document(document).await else {
                continue;
            };
            return Some(Absences {
                absences: extracted_absences,
                source_document: document.clone(),
            });
        }
    }
    None
}

async fn extract_absences_from_document(doc: &Document) -> Option<Vec<String>> {
    let mut extractor = TextExtractor::default();
    let text = extractor
        .extract_text_from_url(&doc.link, None, true)
        .await?;
    extract_absences_from_text(&text)
}

fn extract_absences_from_text(text: &str) -> Option<Vec<String>> {
    // remove this later -> only for testing purposes
    // if text.len() < 50000 {
    //     return None;
    // }

    let text = &text.replace('\u{a0}', " ");
    let text = text.replace('\u{ad}', "");
    let titles = unique_titles();

    let text = text
        .split_inclusive(char::is_whitespace)
        .filter(|x| !titles.contains(x.trim_matches(char::is_whitespace)))
        .filter(|x| !(x.contains(".") && x.len() <= 3))
        .collect::<String>();

    let absence_marker_regex = Regex::new(r"verhindert\s*gemeldet").unwrap();
    let absence_match = absence_marker_regex.find(&text)?;

    let mut start_idx = absence_match.start();

    const STOP_CHARS: [u8; 4] = [b'.', b'!', b'\n', b'?'];

    // should handle these "Ich gebe bekannt, dass Frau Dr. Mertel, Frau Dr. Partik-Pablé, Frau Dr. Povysil, Herr Dr. Puttinger und Herr Ing. Maderthaner für die heutige Sitzung als verhindert gemeldet sind."
    while !STOP_CHARS.contains(&text.as_bytes()[start_idx]) {
        start_idx -= 1;
    }

    let open_ended_absence = &text[start_idx + 1..];
    // println!("open_ended_absence: {}", &open_ended_absence[..500]);

    // let print = &text[start_idx..start_idx + 400];
    // println!("print: {print}");

    // open_ended_absence.replace(&["Hi", "Bi"], " ");

    let info = &open_ended_absence[..open_ended_absence
        .find(".\n")
        .unwrap_or(usize::MAX)
        .min(open_ended_absence.find(". \n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(".\r\n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(". \r\n").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(".  \n").unwrap_or(usize::MAX))
        // need to find the specific absence entry (again)
        // ? /dokument/XXV/NRSITZ/88/fnameorig_501525.html
        .min(open_ended_absence.find(". (Abg").unwrap_or(usize::MAX))
        .min(open_ended_absence.find(", die ich").unwrap_or(usize::MAX))
        .min(
            open_ended_absence
                .find(", Zuweisungen")
                .unwrap_or(usize::MAX),
        )
        .min(open_ended_absence.find(".").unwrap_or(usize::MAX))
        .min(
            open_ended_absence
                .find(". (Ruf bei der ÖVP: ... alle von der SPÖ!)")
                .unwrap_or(usize::MAX),
        )
        .min(
            open_ended_absence
                .find(". (s. auch S. 32)")
                .unwrap_or(open_ended_absence.len()),
        )];

    log::info!(
        "-------------
    info: {info:?}"
    );

    let mut skip_words = [
        "dass",
        "MMSc",
        "diplômé",
        "verhindert",
        "gemeldet",
        "gemeldet:",
        "Bundesrates",
        "für",
        "die",
        "heutige",
        "Sitzung",
        "ist",
        "niemand",
        "sind",
        "Mitglieder",
        "Hohen",
        "Hauses",
        "Abgeordnete:",
        "Abgeordnete,",
        "Abgeordneten",
        "Abgeordneten:",
        "heute",
        "Abgeordneter",
        "Abgeordnete",
        "Dritte",
        "Herr",
        "Nationalrates",
        "Präsident",
        "Präsidentin",
        "unsere",
        "leider",
        "erkrankte",
        "weiters",
        "auch",
        "Zweite",
        "heutigen",
        "Sitzungstag",
        "Sitzungen",
        "Frau",
        "gebe",
        "bekannt",
        "folgende",
        "diesem",
        "Sinne",
        "Pilz:",
        "doch",
        "nicht",
        "Kickl!",
        "keine",
        "Klubvorsitzende",
        "Klubvorsitzender",
        "weitere",
        "Zwischenrufe",
        "Zwischenruf",
        "(Rufe",
        "Heiterkeit",
        "wird",
        "immer",
        "automatisch",
        "entschuldigt!)",
        "Kollege",
        "herzlichen",
        "Dank!",
        "sollten",
        "darüber",
        "unterhalten",
        "warum",
        "fehlend",
        "eingetragen",
        "Grünen)",
        "insgesamt",
        "42",
        "nämlich",
        "ganze",
        "vertreten",
        "durch",
        "Vizekanzler",
        "entschuldigt",
        "damit",
        "Kollegen",
        "Kolleginnen",
        "einige",
        "nämlich",
        "Dipl",
        "heute",
        "und",
        "sowie",
    ]
    .into_iter()
    .collect::<HashSet<_>>();

    skip_words.extend(
        unique_titles()
            .iter()
            .flat_map(|title| title.split('.'))
            .map(|title| title.trim_matches(char::is_whitespace)),
    );

    let info = info
        .split(',')
        .flat_map(|chunk| {
            let words: Vec<&str> = chunk.split_whitespace().collect();
            words
                .split(|w: &&str| *w == "und" || *w == "sowie")
                .map(|group| group.join(" "))
                .collect::<Vec<_>>()
        })
        .filter(|s| !s.is_empty())
        .map(|x| {
            x.split_inclusive(char::is_whitespace)
                .filter(|split| {
                    !(split.chars().filter(|x| x.is_alphabetic()).count() <= 3
                        || split.contains('.')
                        || {
                            let mut ret = false;

                            if skip_words.contains(split.trim_matches(char::is_whitespace)) {
                                ret = true;
                            }
                            ret
                        })
                })
                .collect::<String>()
        })
        // .map(|x| x.collect::<String>())
        .collect::<Vec<_>>();

    Some(
        filter_name_extraction(info.iter().map(|x| x.as_str()))
            .filter(|x| !x.is_empty())
            .collect::<Vec<_>>(),
    )
}

#[cfg(test)]
mod tests {
    use common_scrapes::Document;

    use crate::plenar::absences::extract_absences_from_document;

    #[tokio::test]
    async fn test_absences_extraction_from_doc() {
        let link = "https://www.parlament.gv.at/dokument/XXIII/NRSITZ/72/fnameorig_144783.html"
            .to_string();
        let doc = Document {
            title: "nix".into(),
            link,
            ftype: common_scrapes::FileType::HTML,
        };

        let absences = extract_absences_from_document(&doc).await.unwrap();
        println!("absences: {absences:?}");
        assert_eq!(&absences, &Vec::<String>::new());
    }

    //
    #[tokio::test]
    async fn test_absences_extraction_from_doc2() {
        let link =
            "https://www.parlament.gv.at/dokument/XXIV/NRSITZ/1/fnameorig_144815.html".to_string();
        let doc = Document {
            title: "nix".into(),
            link,
            ftype: common_scrapes::FileType::HTML,
        };

        let absences = extract_absences_from_document(&doc).await.unwrap();
        assert_eq!(absences, vec!["Wilhelm Molterer".to_string()]);
        println!("absences: {absences:?}");
    }
}
