//! Full Nationalrat response parsing, including cached stenographic documents.

use chrono::NaiveDate;
use somes_scraper::{
    plenar::{Plenar, PlenarInfo},
    AustriaScraper, Reference,
};

async fn plenary(source: &str) -> PlenarInfo {
    let mut cache = AustriaScraper::default().cache_config;
    cache.cache_dir = concat!(env!("CARGO_MANIFEST_DIR"), "/test-data").into();
    cache.ignore_current_gp_cache = false;
    cache.redo_secs = u64::MAX;
    let raw = serde_json::from_str(source).expect("valid parliament JSON fixture");
    AustriaScraper::raw_plenar_into_plenar_info(raw, &cache)
        .await
        .expect("fixture must parse as a plenary session")
}

#[tokio::test]
async fn short_session_preserves_metadata_dates_and_absences() {
    let session = plenary(include_str!("../test-data/gegenstand/XXVIII/NRSITZ/6.json")).await;
    let session_date = NaiveDate::from_ymd_opt(2024, 12, 11).unwrap();

    assert_eq!(session.gp, "XXVIII");
    assert_eq!(session.council, "NR");
    assert_eq!(session.ityp, "NRSITZ");
    assert_eq!(session.inr, 6);
    assert_eq!(
        session.title,
        "6. Sitzung des Nationalrats im Anschluss an die 5. NR-Sitzung"
    );
    assert_eq!(
        session.description,
        "6. Sitzung des Nationalrats im Anschluss an die 5. NR-Sitzung (6/NRSITZ)"
    );
    assert_eq!(session.created_at.to_rfc3339(), "2024-12-10T23:00:00+00:00");
    assert_eq!(
        session.updated_at.unwrap().to_rfc3339(),
        "2026-04-27T12:42:46+00:00"
    );
    assert_eq!(session.plenary_dates, [session_date]);
    assert_eq!(session.agenda_dates, [session_date]);
    assert!(session.debates.is_empty());
    assert!(session.call_to_orders.is_empty());

    assert_eq!(session.absences.len(), 1);
    assert_eq!(session.absences[0].date, session_date);
    let names: Vec<_> = session.absences[0]
        .absences
        .iter()
        .map(|absence| absence.name.as_str())
        .collect();
    assert_eq!(
        names,
        [
            "Marie-Christine Giuliani-Sterrer",
            "Werner Herbert",
            "Daniela Gmeinbauer",
            "Olga Voglauer",
            "Alma Zadić"
        ]
    );
    assert!(session.absences[0]
        .source_document
        .link
        .starts_with("https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/6/"));
}

#[tokio::test]
async fn debate_preserves_references_speaker_order_and_durations() {
    let session = plenary(include_str!(
        "../test-data/gegenstand/XXVIII/NRSITZ/73.json"
    ))
    .await;
    // The source also contains a question hour without speeches.
    assert_eq!(session.debates.len(), 6);
    assert!(session
        .debates
        .iter()
        .all(|debate| !debate.speeches.is_empty()));
    let debate = session
        .debates
        .iter()
        .find(|debate| debate.info.about == "Nationaler Aktionsplan gegen Gewalt an Frauen")
        .expect("expected debate must not disappear during parsing");

    assert_eq!(
        debate.info.references,
        [
            Reference {
                gp: "XXVIII".into(),
                ityp: "I".into(),
                inr: 424
            },
            Reference {
                gp: "XXVIII".into(),
                ityp: "I".into(),
                inr: 425
            },
        ]
    );
    assert_eq!(debate.speeches.len(), 26);
    let first = &debate.speeches[0];
    let last = &debate.speeches[25];
    assert_eq!(first.speech.delegate_id, 87002);
    assert_eq!(last.speech.delegate_id, 32563);
    assert_eq!(first.duration_in_seconds, Some(277));
    assert_eq!(last.duration_in_seconds, Some(120));
    assert!(first.speech.delegate_display_name.contains("Rosa Ecker"));
    assert!(last.speech.delegate_display_name.contains("Janos Juvan"));
    for speech in &debate.speeches {
        assert_eq!(
            speech.speech.spoken_text_html_urls.len(),
            1,
            "{}",
            speech.speech.delegate_display_name
        );
        assert!(speech.speech.spoken_text_html_urls[0]
            .document_url
            .starts_with("https://www.parlament.gv.at/dokument/XXVIII/NRSITZ/73/"));
    }
}
