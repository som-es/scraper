use super::*;

const SITTING_2024_01_15: &str = r#"{
  "id": "eli/dl/event/MTG-PL-2024-01-15",
  "activity_id": "MTG-PL-2024-01-15",
  "activity_date": "2024-01-15",
  "activity_start_date": "2024-01-15T01:00:00+01:00",
  "activity_end_date": "2024-01-15T23:00:00+01:00",
  "parliamentary_term": "org/ep-9",
  "hasLocality": "http://publications.europa.eu/resource/authority/place/FRA_SXB",
  "number_of_attendees": 581,
  "had_activity_type": "def/ep-activities/PLENARY_SITTING",
  "activity_label": { "en": "Monday, 15 January 2024", "de": "Montag, 15. Januar 2024" },
  "had_participant_person": ["person/99283", "person/88882", "person/238639"],
  "had_excused_person": ["person/197683", "person/197579"],
  "documented_by_a_realization_of": ["eli/dl/doc/OJQ-9-2024-01-15"],
  "recorded_in_a_realization_of": [
    "eli/dl/doc/PV-9-2024-01-15-ATT",
    "eli/dl/doc/PV-9-2024-01-15-RCV",
    "eli/dl/doc/PV-9-2024-01-15-VOT"
  ],
  "consists_of": [
    "eli/dl/event/MTG-PL-2024-01-15-PVCRE-ITM-14",
    "eli/dl/event/MTG-PL-2024-01-15-PVCRE-CHA-2",
    "eli/dl/event/MTG-PL-2024-01-15-VOT-ITM-000003",
    "eli/dl/event/MTG-PL-2024-01-15-VOT-ITM-150124",
    "eli/dl/event/MTG-PL-2024-01-15-PVCRE-ITM-1"
  ]
}"#;

const SPEECH_2024_01_15_OTH_10850000: &str = r#"{
  "id": "eli/dl/event/MTG-PL-2024-01-15-OTH-10850000",
  "activity_id": "MTG-PL-2024-01-15-OTH-10850000",
  "activity_date": "2024-01-15",
  "activity_start_date": "2024-01-15T18:23:48.774+01:00",
  "activity_end_date": "2024-01-15T18:25:31+01:00",
  "had_activity_type": "def/ep-activities/PLENARY_DEBATE_SPEECH",
  "activity_label": { "en": "Ozone depleting substances - Fluorinated gases regulation (joint debate - Gas emissions)" },
  "had_participation": {
    "id": "eli/dl/participation/MTG-PL-2024-01-15-OTH-10850000_238639",
    "had_participant_person": ["person/238639"],
    "participation_in_name_of": "org/ECR",
    "participation_role": "def/ep-roles/SPEAKER"
  },
  "recorded_in_a_realization_of": [
    {
      "id": "eli/dl/doc/CRE-9-2024-01-15-OTH-10850000",
      "notation_speechId": "10850000",
      "originalLanguage": ["http://publications.europa.eu/resource/authority/language/SLK"]
    }
  ],
  "inverse_consists_of": ["eli/dl/event/MTG-PL-2024-01-15-PVCRE-ITM-14"]
}"#;

const DEBATE_ITEM_2024_01_15_ITM_14: &str = r#"{
  "id": "eli/dl/event/MTG-PL-2024-01-15-PVCRE-ITM-14",
  "activity_id": "MTG-PL-2024-01-15-PVCRE-ITM-14",
  "activity_date": "2024-01-15",
  "had_activity_type": "def/ep-activities/PLENARY_DEBATE",
  "activity_label": { "en": "Ozone depleting substances - Fluorinated gases regulation (joint debate - Gas emissions)" },
  "based_on_a_realization_of": [
    "eli/dl/doc/A-9-2023-0050",
    "eli/dl/doc/C-9-2022-0142",
    "eli/dl/doc/A-9-2023-0048"
  ],
  "consists_of": [
    "eli/dl/event/MTG-PL-2024-01-15-OTH-10850000",
    "eli/dl/event/MTG-PL-2024-01-15-OTH-10800000"
  ]
}"#;

const VOTE_ITEM_2024_01_15_VOT_000003: &str = r#"{
  "id": "eli/dl/event/MTG-PL-2024-01-15-VOT-ITM-000003",
  "activity_id": "MTG-PL-2024-01-15-VOT-ITM-000003",
  "activity_date": "2024-01-15",
  "had_activity_type": "def/ep-activities/PLENARY_VOTE_RESULTS",
  "notation_dlvId": "000003",
  "consists_of": ["eli/dl/event/MTG-PL-2024-01-15-DEC-163201"]
}"#;

const ATTENDANCE_DOC_2024_01_15: &str = r#"{
  "id": "eli/dl/doc/PV-9-2024-01-15-ATT",
  "title_dcterms": { "en": "Minutes - Attendance List - Monday, 15 January 2024" },
  "is_realized_by": [
    {
      "id": "eli/dl/doc/PV-9-2024-01-15-ATT/fr",
      "is_embodied_by": [
        {
          "format": "http://publications.europa.eu/resource/authority/file-type/PDF",
          "is_exemplified_by": "distribution/reds_iPlPv_Att/PV-9-2024-01-15-ATT/PV-9-2024-01-15-ATT-FNL_fr.pdf"
        }
      ]
    },
    {
      "id": "eli/dl/doc/PV-9-2024-01-15-ATT/en",
      "is_embodied_by": [
        {
          "format": "http://publications.europa.eu/resource/authority/file-type/DOCX",
          "is_exemplified_by": "distribution/reds_iPlPv_Att/PV-9-2024-01-15-ATT/PV-9-2024-01-15-ATT-FNL_en.docx"
        },
        {
          "format": "http://publications.europa.eu/resource/authority/file-type/PDF",
          "is_exemplified_by": "distribution/reds_iPlPv_Att/PV-9-2024-01-15-ATT/PV-9-2024-01-15-ATT-FNL_en.pdf"
        },
        {
          "format": "http://publications.europa.eu/resource/authority/file-type/XML",
          "is_exemplified_by": "distribution/reds_iPlPv_Att/PV-9-2024-01-15-ATT/PV-9-2024-01-15-ATT-FNL_en.xml"
        }
      ]
    }
  ]
}"#;

fn json(raw: &str) -> Value {
    serde_json::from_str(raw).unwrap()
}

#[test]
fn parse_sitting_2024_01_15() {
    let sitting = parse_plenary_sitting(&json(SITTING_2024_01_15)).unwrap();

    assert_eq!(sitting.id, "MTG-PL-2024-01-15");
    assert_eq!(sitting.date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    assert_eq!(sitting.term.as_deref(), Some("9"));
    assert_eq!(sitting.title.as_deref(), Some("Monday, 15 January 2024"));
    assert_eq!(sitting.locality.as_deref(), Some("FRA_SXB"));
    assert_eq!(sitting.number_of_attendees, Some(581));
    assert_eq!(
        sitting.start.unwrap().to_rfc3339(),
        "2024-01-15T01:00:00+01:00"
    );
    assert_eq!(sitting.attendee_ids, vec![99283, 88882, 238639]);
    assert_eq!(sitting.excused_ids, vec![197683, 197579]);
    assert_eq!(
        sitting.debate_item_ids,
        vec![
            "MTG-PL-2024-01-15-PVCRE-ITM-14",
            "MTG-PL-2024-01-15-PVCRE-ITM-1"
        ]
    );
    assert_eq!(
        sitting.vote_item_ids,
        vec![
            "MTG-PL-2024-01-15-VOT-ITM-000003",
            "MTG-PL-2024-01-15-VOT-ITM-150124"
        ]
    );
    assert_eq!(sitting.agenda_doc_ids, vec!["OJQ-9-2024-01-15"]);
    assert_eq!(
        sitting.attendance_doc_id.as_deref(),
        Some("PV-9-2024-01-15-ATT")
    );
}

#[test]
fn parse_speech_2024_01_15_oth_10850000() {
    let speech = parse_speech(&json(SPEECH_2024_01_15_OTH_10850000)).unwrap();

    assert_eq!(speech.id, "MTG-PL-2024-01-15-OTH-10850000");
    assert_eq!(
        speech.sitting_date,
        NaiveDate::from_ymd_opt(2024, 1, 15).unwrap()
    );
    assert_eq!(speech.speaker_id, Some(238639));
    assert_eq!(speech.speaker_group.as_deref(), Some("ECR"));
    assert_eq!(speech.role.as_deref(), Some("SPEAKER"));
    assert_eq!(speech.original_language.as_deref(), Some("SLK"));
    assert_eq!(
        speech.verbatim_doc_id.as_deref(),
        Some("CRE-9-2024-01-15-OTH-10850000")
    );
    assert_eq!(
        speech.agenda_item_id.as_deref(),
        Some("MTG-PL-2024-01-15-PVCRE-ITM-14")
    );
    assert_eq!(
        speech.start.unwrap().to_rfc3339(),
        "2024-01-15T18:23:48.774+01:00"
    );
}

#[test]
fn parse_debate_item_2024_01_15_itm_14() {
    let item = parse_agenda_item(&json(DEBATE_ITEM_2024_01_15_ITM_14)).unwrap();

    assert_eq!(item.id, "MTG-PL-2024-01-15-PVCRE-ITM-14");
    assert_eq!(item.activity_type.as_deref(), Some("PLENARY_DEBATE"));
    assert_eq!(
        item.based_on_references,
        vec!["A9-0050/2023", "C9-0142/2022", "A9-0048/2023"]
    );
    assert_eq!(
        item.speech_ids,
        vec![
            "MTG-PL-2024-01-15-OTH-10850000",
            "MTG-PL-2024-01-15-OTH-10800000"
        ]
    );
    assert!(item.vote_ids.is_empty());
}

#[test]
fn parse_vote_item_2024_01_15_vot_000003() {
    let item = parse_agenda_item(&json(VOTE_ITEM_2024_01_15_VOT_000003)).unwrap();

    assert_eq!(item.id, "MTG-PL-2024-01-15-VOT-ITM-000003");
    assert_eq!(item.activity_type.as_deref(), Some("PLENARY_VOTE_RESULTS"));
    assert!(item.based_on_references.is_empty());
    assert!(item.speech_ids.is_empty());
    assert_eq!(item.vote_ids, vec!["MTG-PL-2024-01-15-DEC-163201"]);
}

#[test]
fn group_speeches_by_agenda_item_2024_01_15() {
    let speeches = vec![
        Speech {
            id: "MTG-PL-2024-01-15-OTH-1".into(),
            sitting_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            title: None,
            speaker_id: Some(1),
            speaker_group: None,
            role: None,
            start: None,
            end: None,
            verbatim_doc_id: None,
            original_language: None,
            agenda_item_id: Some("MTG-PL-2024-01-15-PVCRE-ITM-14".into()),
            is_part_of: None,
            numbering: 0,
            speech_texts: vec![],
            ty: None,
        },
        Speech {
            id: "MTG-PL-2024-01-15-OTH-2".into(),
            sitting_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            title: None,
            speaker_id: Some(2),
            speaker_group: None,
            role: None,
            start: None,
            end: None,
            verbatim_doc_id: None,
            original_language: None,
            agenda_item_id: Some("MTG-PL-2024-01-15-PVCRE-ITM-14".into()),
            numbering: 0,
            speech_texts: vec![],
            ty: None,
            is_part_of: None,
        },
        Speech {
            id: "MTG-PL-2024-01-15-OTH-3".into(),
            sitting_date: NaiveDate::from_ymd_opt(2024, 1, 15).unwrap(),
            title: None,
            speaker_id: Some(3),
            speaker_group: None,
            role: None,
            start: None,
            end: None,
            is_part_of: None,
            verbatim_doc_id: None,
            original_language: None,
            agenda_item_id: Some("MTG-PL-2024-01-15-PVCRE-ITM-1".into()),
            numbering: 0,
            speech_texts: vec![],
            ty: None,
        },
    ];

    let grouped = speeches_by_agenda_item(&speeches);
    assert_eq!(grouped.len(), 2);
    assert_eq!(grouped[0].0, "MTG-PL-2024-01-15-PVCRE-ITM-14");
    assert_eq!(grouped[0].1.len(), 2);
    assert_eq!(grouped[1].0, "MTG-PL-2024-01-15-PVCRE-ITM-1");
    assert_eq!(grouped[1].1.len(), 1);
}

#[test]
fn sitting_maps_into_common_plenar_info() {
    let sitting = parse_plenary_sitting(&json(SITTING_2024_01_15)).unwrap();
    let info = sitting_to_plenar_info(&sitting);

    assert_eq!(info.gp, "9");
    assert_eq!(info.council, "ep");
    assert_eq!(info.ityp, "PLENARY_SITTING");
    assert_eq!(info.inr, 20240115);
    assert_eq!(
        info.created_at,
        DateTime::parse_from_rfc3339("2024-01-15T01:00:00+01:00")
            .unwrap()
            .with_timezone(&chrono::Utc)
    );
    assert!(info.absences.is_empty());
}

#[test]
fn attendance_document_prefers_english_pdf() {
    let document = attendance_document_from_raw(&json(ATTENDANCE_DOC_2024_01_15)).unwrap();

    assert_eq!(document.title, "attendance");
    assert_eq!(document.ftype, common_scrapes::FileType::DigitalPDF);
    assert_eq!(
        document.link,
        "https://data.europarl.europa.eu/distribution/reds_iPlPv_Att/PV-9-2024-01-15-ATT/PV-9-2024-01-15-ATT-FNL_en.pdf"
    );
}

#[test]
fn report_reference_maps_into_common_reference() {
    let reference = reference_from_report("A9-0050/2023").unwrap();
    assert_eq!(reference.ityp, "A");
    assert_eq!(reference.gp, "9");
    assert_eq!(reference.inr, 20230050);
    assert!(reference_from_report("not-a-ref").is_none());
}

#[test]
fn speech_maps_into_common_plenar_speech() {
    let speech = parse_speech(&json(SPEECH_2024_01_15_OTH_10850000)).unwrap();
    let mapped = speech_to_plenar_speech(speech.clone());

    assert_eq!(mapped.ty, "wm");
    assert_eq!(mapped.speech.delegate_id, 238639);
    assert_eq!(mapped.duration_in_seconds, Some(102));
    assert_eq!(mapped.speech.spoken_text_html_urls.len(), 1);
    assert!(mapped.speech.spoken_text_html_urls[0]
        .document_url
        .ends_with("/documents/CRE-9-2024-01-15-OTH-10850000"));
}

#[tokio::test]
async fn live_get_plenar_2024_01_15() {
    use crate::plenar::Plenar;

    let scraper = EuParliamentScraper::default();
    let info = scraper.get_plenar("MTG-PL-2024-01-15").await.unwrap();

    assert_eq!(info.gp, "9");
    assert_eq!(info.council, "ep");
    assert_eq!(info.inr, 20240115);
    assert!(!info.debates.is_empty());
    assert!(info
        .debates
        .iter()
        .any(|debate| debate.speeches.iter().any(|s| s.speech.delegate_id != 0)));

    assert_eq!(info.absences.len(), 1);

    let absences = &info.absences[0];
    assert!(!absences.absences.is_empty());

    assert_eq!(
        absences.source_document.ftype,
        common_scrapes::FileType::DigitalPDF
    );
    assert!(absences.source_document.link.ends_with("_en.pdf"));

    let debate = &info.debates[0];
    let ps = &debate.speeches[0];
    assert_eq!(ps.ty, "wm".to_string());
    assert_eq!(
        &ps.speech.spoken_text_html_urls[0],
        &SpeechTextInfo {
            site_start: None,
            site_end: None,
            document_url:
                "https://www.europarl.europa.eu/doceo/document/CRE-9-2024-01-15-ITM-010_EN.html"
                    .into()
        }
    );
}

#[tokio::test]
async fn live_get_plenary_sitting_2024_01_15() {
    let scraper = EuParliamentScraper::default();
    let sitting = scraper
        .get_plenary_sitting("MTG-PL-2024-01-15")
        .await
        .unwrap();

    assert_eq!(sitting.date, NaiveDate::from_ymd_opt(2024, 1, 15).unwrap());
    assert_eq!(sitting.term.as_deref(), Some("9"));
    assert!(sitting.number_of_attendees.unwrap() > 100);
    assert!(!sitting.attendee_ids.is_empty());
    assert!(!sitting.debate_item_ids.is_empty());
    assert!(!sitting.vote_item_ids.is_empty());
}

#[tokio::test]
async fn live_get_plenary_sitting_by_date_2024_04_25() {
    let scraper = EuParliamentScraper::default();
    let date = NaiveDate::from_ymd_opt(2024, 4, 25).unwrap();
    let sitting = scraper.get_plenary_sitting_by_date(date).await.unwrap();

    assert_eq!(sitting.id, "MTG-PL-2024-04-25");
    assert_eq!(sitting.date, date);
    assert!(!sitting.vote_item_ids.is_empty());
}

#[tokio::test]
async fn live_get_plenary_sitting_by_date_2019_01_14() {
    let scraper = EuParliamentScraper::default();
    let date = NaiveDate::from_ymd_opt(2019, 1, 14).unwrap();
    let sitting = scraper.get_plenary_sitting_by_date(date).await.unwrap();

    assert_eq!(sitting.id, "MTG-PL-2019-01-14");
    assert_eq!(sitting.date, date);
}

#[tokio::test]
async fn live_get_plenary_sitting_by_date_2026_07_09() {
    let scraper = EuParliamentScraper::default();
    let plenar_info = scraper.get_plenar("MTG-PL-2026-07-09").await.unwrap();

    // find refenrence "A10-0167/2026",
    for debate in &plenar_info.debates {
        dbg!(&debate.info);
    }
    // assert!(plenar_info.debates.iter().any(|debate| debate
    //     .info
    //     .references
    //     .iter()
    //     .any(|val| val..contains("A10-0167/2026"))));
}

#[tokio::test]
async fn live_get_plenary_sitting_by_date_2026_07_06() {
    let scraper = EuParliamentScraper::default();
    let plenar_info = scraper.get_plenar("MTG-PL-2026-07-06").await.unwrap();

    // find refenrence "A10-0167/2026",
    for debate in &plenar_info.debates {
        if debate.info.about.contains("Air passenger") {
            dbg!(debate);
        }
    }
    // assert!(plenar_info.debates.iter().any(|debate| debate
    //     .info
    //     .references
    //     .iter()
    //     .any(|val| val..contains("A10-0167/2026"))));
}

#[tokio::test]
async fn live_get_sitting_speeches_2024_01_15() {
    let scraper = EuParliamentScraper::default();
    let date = NaiveDate::from_ymd_opt(2024, 1, 15).unwrap();
    let speeches = scraper.get_sitting_speeches(date).await.unwrap();

    assert!(!speeches.is_empty());
    assert!(speeches.iter().all(|speech| speech.sitting_date == date));
    assert!(speeches.iter().any(|speech| speech.speaker_id.is_some()));
    assert!(speeches
        .iter()
        .any(|speech| speech.agenda_item_id.is_some()));
    assert!(speeches
        .iter()
        .any(|speech| speech.verbatim_doc_id.is_some()));
}

#[tokio::test]
async fn live_get_agenda_item_2024_01_15_itm_14() {
    let scraper = EuParliamentScraper::default();
    let item = scraper
        .get_agenda_item("MTG-PL-2024-01-15-PVCRE-ITM-14")
        .await
        .unwrap();

    assert_eq!(item.activity_type.as_deref(), Some("PLENARY_DEBATE"));
    assert!(!item.speech_ids.is_empty());
    assert!(item.based_on_references.iter().any(|r| r.contains("/2023")));
}

#[tokio::test]
async fn live_list_plenary_sitting_ids_2024() {
    let scraper = EuParliamentScraper::default();
    let ids = scraper.list_plenary_sitting_ids(2024).await.unwrap();

    assert!(!ids.is_empty());
    assert!(ids.iter().all(|id| id.starts_with("MTG-PL-2024-")));
    assert!(ids.contains(&"MTG-PL-2024-01-15".to_string()));
    assert!(ids.windows(2).all(|w| w[0] < w[1]));
}
