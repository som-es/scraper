//! Parser contracts: fixed API fixtures and malformed-input cases, no live requests.
use chrono::NaiveDate;
use serde_json::{json, Value};
use somes_scraper::{
    call_to_order::{CallToOrder, CallToOrderError},
    dates::{Dates, Gremium, Terminart},
    delegates::DelegateScraper,
    enactment::EnactmentScraper,
    gov_officials::extract_gov_official,
    ministrial_proposals::{GegenstandInfo, MinistrialProposalScraper},
    oral_inquiries::{ExchangeKind, OralInquiryScraper},
    parliamentary_questions::ParliamentaryQAScraper,
    proposals::ProposalScraper,
    AustriaScraper, FileType, QaType, Reference, TextExtractor,
};

const MINISTERIAL: &str = include_str!("../test-data/gegenstand/XXV/ME/209.json");
const QUESTION: &str = include_str!("../test-data/gegenstand/XXVII/J/14815.json");
const ORAL: &str = include_str!("../test-data/gegenstand/XXVII/M/384.json");

fn raw(source: &str) -> Value {
    serde_json::from_str(source).unwrap()
}
fn info() -> GegenstandInfo {
    GegenstandInfo {
        path: "/gegenstand/XXV/ME/209".into(),
        ressort: "Bundeskanzleramt".into(),
        ressort_shortform: "BKA".into(),
        due_to: NaiveDate::from_ymd_opt(2016, 5, 30).unwrap(),
    }
}

#[test]
fn ministerial_preserves_identity_documents_topics_and_law_reference() {
    let p =
        AustriaScraper::raw_ministrial_prop_into_ministrial_prop(raw(MINISTERIAL), info()).unwrap();
    assert_eq!(
        (p.id, p.inr, p.ityp.as_str(), p.gp.as_str()),
        (209, 209, "ME", "XXV")
    );
    assert_eq!(
        p.title,
        "Dienstrechts-Novelle 2016 (209/ME) | Parlament Österreich"
    );
    assert_eq!(p.gegenstand_info, info());
    assert!(p.delegate_ids.is_empty()); // This historical record names a ministry, not a person.
    assert_eq!(p.emphasis, None);
    assert_eq!(p.topics, ["Inneres und Recht"]);
    assert_eq!(
        p.eurovoc_topics,
        ["öffentliche Verwaltung", "öffentlicher Dienst"]
    );
    assert_eq!(p.created_at.to_rfc3339(), "2016-05-17T22:00:00+00:00");
    assert_eq!(
        p.updated_at.unwrap().to_rfc3339(),
        "2026-08-13T21:03:40+00:00"
    );
    assert_eq!(
        p.legis_reference,
        Some(Reference {
            gp: "XXV".into(),
            ityp: "I".into(),
            inr: 1188
        })
    );
    assert_eq!(p.documents.len(), 9);
    assert_eq!(
        p.documents[0].link,
        "https://www.parlament.gv.at/dokument/XXV/ME/209/fname_532522.pdf"
    );
    assert_eq!(p.documents[0].ftype, FileType::DigitalPDF);
    assert_eq!(p.documents[1].ftype, FileType::HTML);
    assert_eq!(
        p.documents[8].title,
        "Begleitschreiben Begutachtungsentwurf"
    );
}

#[test]
fn ministerial_extracts_person_and_decodes_shortinfo() {
    let p = AustriaScraper::raw_ministrial_prop_into_ministrial_prop(
        raw(include_str!("../test-data/gegenstand/XXVIII/ME/53.json")),
        info(),
    )
    .unwrap();
    assert_eq!(p.delegate_ids, [32508]);
    let emphasis = p.emphasis.unwrap();
    assert!(emphasis.contains("Erlassung des Bundesstraßennotfallgesetzes"));
    assert!(emphasis.contains("Hauptgesichtspunkte des Entwurfs"));
    assert!(!emphasis.contains("<h4>"));
    assert!(!emphasis.contains("&szlig;"));
}

#[test]
fn ministerial_rejects_missing_required_fields_without_panicking() {
    for pointer in [
        "/content/ityp",
        "/content/inr",
        "/content/gp_code",
        "/content/description",
        "/content/einlangen",
        "/meta/title",
    ] {
        let mut v = raw(MINISTERIAL);
        *v.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            AustriaScraper::raw_ministrial_prop_into_ministrial_prop(v, info()).is_none(),
            "{pointer}"
        );
    }
}

#[test]
fn ministerial_optional_update_can_be_absent_or_invalid() {
    for update in [Value::Null, json!("not a timestamp")] {
        let mut v = raw(MINISTERIAL);
        v["content"]["update"] = update;
        let p = AustriaScraper::raw_ministrial_prop_into_ministrial_prop(v, info()).unwrap();
        assert_eq!(p.updated_at, None);
    }
}

#[test]
fn ministerial_handles_no_documents_and_no_subsequent_law() {
    let mut v = raw(MINISTERIAL);
    v["content"]["documents"] = json!([]);
    v["content"]["stages"] = json!([]);
    let p = AustriaScraper::raw_ministrial_prop_into_ministrial_prop(v, info()).unwrap();
    assert!(p.documents.is_empty());
    assert_eq!(p.legis_reference, None);
}

#[test]
fn parliamentary_question_preserves_people_and_answer_reference() {
    let v = raw(QUESTION);
    let p = AustriaScraper::raw_proposal_into_proposal(v.clone()).unwrap();
    let qa = AustriaScraper::raw_value_into_parliamentary_qa(v, p).unwrap();
    assert_eq!(qa.qa_type, QaType::Question);
    assert_eq!(qa.issuer_ids, [2344]);
    assert_eq!(qa.receiver_ids, [20445]);
    assert_eq!(qa.references.len(), 1);
    assert_eq!(qa.references[0].qa_type, QaType::Answer);
    assert_eq!(
        qa.references[0].reference,
        Reference {
            gp: "XXVII".into(),
            ityp: "AB".into(),
            inr: 14326
        }
    );
}

#[test]
fn parliamentary_answer_preserves_all_five_question_references() {
    let v = raw(include_str!("../test-data/gegenstand/XXVIII/AB/2565.json"));
    let p = AustriaScraper::raw_proposal_into_proposal(v.clone()).unwrap();
    let qa = AustriaScraper::raw_value_into_parliamentary_qa(v, p).unwrap();
    assert_eq!(qa.qa_type, QaType::Answer);
    assert_eq!(qa.receiver_ids, [20445]);
    assert_eq!(
        qa.references
            .iter()
            .map(|r| r.reference.inr)
            .collect::<Vec<_>>(),
        [3019, 3020, 3021, 3022, 3023]
    );
    assert!(qa.references.iter().all(|r| r.qa_type == QaType::Question
        && r.reference.gp == "XXVIII"
        && r.reference.ityp == "J"));
}

#[test]
fn parliamentary_qa_ignores_invalid_references_and_rejects_unrelated_types() {
    let mut v = raw(QUESTION);
    v["content"]["reference"] =
        json!([{}, {"url": false}, {"url": "/gegenstand/XXVII/A/1"}, {"url": "invalid"}]);
    let mut p = AustriaScraper::raw_proposal_into_proposal(v.clone()).unwrap();
    assert!(
        AustriaScraper::raw_value_into_parliamentary_qa(v.clone(), p.clone())
            .unwrap()
            .references
            .is_empty()
    );
    p.ityp = "A".into();
    assert!(AustriaScraper::raw_value_into_parliamentary_qa(v, p).is_none());
}

#[test]
fn proposal_skips_receivers_without_valid_person_urls() {
    let mut v = raw(QUESTION);
    v["content"]["names"] = json!([
        {"funktext":"Eingebracht an"}, {"funktext":"Eingebracht an","url":"invalid"},
        {"funktext":"Eingebracht an","url":"/person/20445"}
    ]);
    assert_eq!(
        AustriaScraper::raw_proposal_into_proposal(v)
            .unwrap()
            .receiver_ids,
        [20445]
    );
}

#[test]
fn oral_inquiry_preserves_exchange_order_and_protocol_pages() {
    let p = AustriaScraper::raw_value_into_oral_inquiry(raw(ORAL)).unwrap();
    assert_eq!(
        (p.inr, p.asker_id, p.recipient_id, p.plenary_session),
        (384, 83059, 2136, Some(274))
    );
    assert_eq!(
        p.exchanges
            .iter()
            .map(|e| (&e.kind, e.person_id))
            .collect::<Vec<_>>(),
        [
            (&ExchangeKind::MainQuestion, Some(83059)),
            (&ExchangeKind::MainAnswer, Some(2136)),
            (&ExchangeKind::FollowUpQuestion, Some(35468)),
            (&ExchangeKind::FollowUpQuestion, Some(5672)),
            (&ExchangeKind::FollowUpAnswerWritten, None)
        ]
    );
    let r = &p.exchanges[0].protocol_refs[0];
    assert_eq!(
        (r.session_id, r.page_from, r.page_to),
        (274, Some(76), Some(82))
    );
    assert_eq!(p.references.len(), 1);
    assert_eq!(p.references[0].art, "ZU");
    assert!(!p.question_text.contains("<br"));
}

#[test]
fn oral_inquiry_supports_old_pages_and_new_anchors() {
    for (source, id, asker, recipient, session, page) in [
        (
            include_str!("../test-data/gegenstand/XXII/M/171.json"),
            171,
            8241,
            1546,
            160,
            Some(27),
        ),
        (
            include_str!("../test-data/gegenstand/XXVIII/M/90.json"),
            90,
            5650,
            2309,
            79,
            None,
        ),
    ] {
        let p = AustriaScraper::raw_value_into_oral_inquiry(raw(source)).unwrap();
        assert_eq!(
            (p.inr, p.asker_id, p.recipient_id, p.plenary_session),
            (id, asker, recipient, Some(session))
        );
        let r = &p.exchanges[0].protocol_refs[0];
        assert_eq!(r.page_from, page);
        if page.is_none() {
            assert!(r.url.ends_with("#8"));
        }
    }
}

#[test]
fn oral_inquiry_handles_missing_optional_and_malformed_stage_data() {
    let mut v = raw(ORAL);
    for f in [
        "reference",
        "topics",
        "eurovoc",
        "headwords",
        "update",
        "description",
    ] {
        v["content"].as_object_mut().unwrap().remove(f);
    }
    v["content"]["stages"] = json!([{}, {"text":false}, {"text":"irrelevant"}, {"text":"Zusatzfrage schriftlich beantwortet","fsth":[{}]}]);
    let p = AustriaScraper::raw_value_into_oral_inquiry(v).unwrap();
    assert_eq!(p.question_text, "");
    assert_eq!(p.updated_at, None);
    assert_eq!(p.plenary_session, None);
    assert!(p.references.is_empty());
    assert!(p.topics.is_empty());
    assert_eq!(p.exchanges.len(), 1);
    assert!(p.exchanges[0].protocol_refs.is_empty());
    assert_eq!(p.exchanges[0].kind, ExchangeKind::FollowUpAnswerWritten);
}

#[test]
fn oral_inquiry_rejects_missing_required_fields() {
    for pointer in [
        "/content/inr",
        "/content/gp_code",
        "/meta/title",
        "/content/names",
        "/content/einlangen",
    ] {
        let mut v = raw(ORAL);
        *v.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            AustriaScraper::raw_value_into_oral_inquiry(v).is_none(),
            "{pointer}"
        );
    }
}

fn date_row() -> Value {
    json!([
        "15.07.2026",
        "202607150900",
        "09:00",
        "Sitzung",
        "/sitzung",
        "Plenarsitzung",
        null,
        null,
        "Wien",
        "[\"Budget\"]",
        "Nationalrat",
        null,
        null,
        null,
        "J",
        "2026-07-15T09:00:00",
        "2026-07-15T10:00:00",
        "N"
    ])
}

#[tokio::test]
async fn dates_preserve_local_time_and_convert_summer_and_winter_to_utc() {
    let mut winter = date_row();
    winter[0] = json!("15.01.2026");
    winter[1] = json!("202601150900");
    winter[15] = json!("2026-01-15T09:00:00");
    let dates = AustriaScraper::default()
        .raw_dates_into_parliament_dates(json!({"rows":[date_row(),winter]}))
        .await
        .unwrap();
    assert_eq!(dates.len(), 2);
    assert_eq!(dates[0].title, "Sitzung");
    assert_eq!(dates[0].topics, ["Budget"]);
    assert_eq!(dates[0].time.unwrap().to_string(), "09:00:00");
    assert_eq!(
        dates[0].start.unwrap().to_rfc3339(),
        "2026-07-15T07:00:00+00:00"
    );
    assert_eq!(
        dates[1].start.unwrap().to_rfc3339(),
        "2026-01-15T08:00:00+00:00"
    );
    assert_eq!(dates[0].date_sort, dates[0].start);
    assert_eq!(dates[1].date_sort, dates[1].start);
    assert!(dates[0].media_relevant);
    assert!(!dates[0].past);
    assert!(matches!(dates[0].committee, Gremium::Nationalrat));
    assert!(matches!(
        dates[0].appointment_type,
        Terminart::Plenarsitzung
    ));
}

#[tokio::test]
async fn dates_skip_invalid_rows_and_preserve_unknown_categories() {
    let mut v = date_row();
    v[5] = json!("Neue Terminart");
    v[10] = json!("Neues Gremium");
    v[9] = json!("invalid json");
    v[2] = json!("25:99");
    let mut bad = date_row();
    bad[0] = json!("31.02.2026");
    let s = AustriaScraper::default();
    let d = s
        .raw_dates_into_parliament_dates(json!({"rows":[[],bad,v]}))
        .await
        .unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].time, None);
    assert!(d[0].topics.is_empty());
    assert!(matches!(&d[0].committee,Gremium::Other(s) if s=="Neues Gremium"));
    assert!(matches!(&d[0].appointment_type,Terminart::Other(s) if s=="Neue Terminart"));
    assert!(s.raw_dates_into_parliament_dates(json!({})).await.is_none());
    assert!(s
        .raw_dates_into_parliament_dates(json!({"rows":[]}))
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn government_official_rejects_truncated_rows_without_panicking() {
    let s = AustriaScraper::default();
    for row in [
        json!([]),
        json!([1]),
        json!([
            1,
            "government",
            "name",
            "Minister<br/>01.01.2020 - 02.02.2021"
        ]),
    ] {
        assert!(extract_gov_official(&row, &s).await.is_none());
    }
}

#[tokio::test]
async fn government_official_preserves_function_and_term() {
    let p = extract_gov_official(
        &json!([
            65321,
            "Kurz II",
            "Sebastian Kurz",
            "Bundeskanzler<br/>07.01.2020 - 11.10.2021",
            "Bundeskanzler:in",
            "Bundeskanzleramt"
        ]),
        &AustriaScraper::default(),
    )
    .await
    .unwrap();
    assert_eq!(p.delegate_id, 65321);
    assert_eq!(p.government, "Kurz II");
    assert_eq!(p.function, "Bundeskanzler");
    assert_eq!(p.ministry, "Bundeskanzleramt");
    assert_eq!(p.start_date, NaiveDate::from_ymd_opt(2020, 1, 7).unwrap());
    assert_eq!(p.end_date, NaiveDate::from_ymd_opt(2021, 10, 11));
}

#[tokio::test]
async fn enactment_rejects_missing_references_and_stages_without_panicking() {
    let s = AustriaScraper::default();
    for v in [
        json!({}),
        json!({"content":{"reference":[],"stages":[]}}),
        json!({"content":{"reference":[{}],"stages":[]}}),
        json!({"content":{"reference":[{"url":"/unused"}],"stages":[]}}),
        json!({"content":{"reference":[{"url":"/unused"}],"stages":[{}]}}),
    ] {
        assert!(s.raw_enactment_into_enactment(&v).await.is_none());
    }
}

fn call_to_order() -> Value {
    json!({"meta":{"description":"Ordnungsruf"},"content":{"ityp":"RUF","inr":1,"einlangen":"2026-01-15T09:00:00","names":[{"name":"Erika Beispiel","url":"/person/42"}],"reference":[]}})
}

#[test]
fn call_to_order_preserves_receiver_and_accepts_no_reference() {
    let p = AustriaScraper::extract_raw_call_to_order(call_to_order()).unwrap();
    assert_eq!(p.receiver_id, 42);
    assert_eq!(p.receiver, "Erika Beispiel");
    assert_eq!(p.inr, 1);
    assert_eq!(p.links_to, None);
    assert_eq!(p.updated_at, None);
    assert_eq!(
        p.created_at.unwrap().to_rfc3339(),
        "2026-01-15T08:00:00+00:00"
    );
}

#[test]
fn call_to_order_rejects_non_numeric_receiver_without_panicking() {
    let mut v = call_to_order();
    v["content"]["names"][0]["url"] = json!("/person/not-a-number");
    assert_eq!(
        AustriaScraper::extract_raw_call_to_order(v),
        Err(CallToOrderError::ReceiverUrlWrongFormat)
    );
}

#[test]
fn call_to_order_invalid_optional_update_does_not_panic() {
    let mut v = call_to_order();
    v["content"]["update"] = json!("invalid");
    assert_eq!(
        AustriaScraper::extract_raw_call_to_order(v)
            .unwrap()
            .updated_at,
        None
    );
}

#[test]
fn speech_fit_preserves_unknown_speaker_text_and_stops_at_next_speech() {
    let extractor = TextExtractor::default();
    let text = "Ein Beitrag ohne Namensüberschrift.";
    assert_eq!(
        somes_scraper::fit_speech(
            text.into(),
            "Unbekannt",
            &extractor.fit_speech_regexes,
            true
        )
        .as_deref(),
        Some(text)
    );
    let text = "09.00\nErika Beispiel: Grüß Gott!\n09.05\nAndere Person: Zweite Rede.";
    assert_eq!(
        somes_scraper::fit_speech(
            text.into(),
            "Erika Beispiel",
            &extractor.fit_speech_regexes,
            false
        )
        .unwrap()
        .trim(),
        "Erika Beispiel: Grüß Gott!"
    );
}

#[test]
fn delegate_minimal_record_preserves_identity_and_optional_absence() {
    let v = json!({"meta":{"description":"Erika Beispiel"},"content":{"personInfo":{"pad_intern":42,"aktiv":false},"biografie":{"mandatefunktionen":{"mandate":[]}}}});
    let d = AustriaScraper::person_info_into_delegate(v.clone()).unwrap();
    assert_eq!(d.id, 42);
    assert_eq!(d.name, "Erika Beispiel");
    assert!(!d.is_active);
    assert!(d.mandates.is_empty());
    assert_eq!(d.seat, None);
    assert_eq!(d.birthdate, None);
    assert_eq!(d.party, None);
    assert_eq!(d.contact.mail, None);
    for ptr in [
        "/meta/description",
        "/content/personInfo/pad_intern",
        "/content/personInfo/aktiv",
    ] {
        let mut bad = v.clone();
        *bad.pointer_mut(ptr).unwrap() = Value::Null;
        assert!(
            AustriaScraper::person_info_into_delegate(bad).is_none(),
            "{ptr}"
        );
    }
}

#[test]
fn proposal_optional_update_can_be_missing_or_invalid() {
    for update in [
        Value::Null,
        json!("not a timestamp"),
        json!("2026-03-29T02:30:00"),
    ] {
        let mut v = raw(QUESTION);
        v["content"]["update"] = update;
        assert_eq!(
            AustriaScraper::raw_proposal_into_proposal(v)
                .unwrap()
                .updated_at,
            None
        );
    }
}

#[tokio::test]
async fn enactment_preserves_nr_and_optional_br_dates_with_fixture_reference() {
    let mut s = AustriaScraper::default();
    s.cache_config.cache_dir = "./test-data".into();
    s.cache_config.redo_secs = u64::MAX;
    s.cache_config.ignore_current_gp_cache = false;
    s.text_extractor.cache_config = s.cache_config.clone();
    for br in [false, true] {
        let mut v = json!({"content":{"reference":[{"url":"/gegenstand/XXVII/A/3373"}],"stages":[{"date":"25.05.2023"}]}});
        if br {
            v["content"]["stages"]
                .as_array_mut()
                .unwrap()
                .push(json!({"date":"07.06.2023"}));
        }
        let e = s.raw_enactment_into_enactment(&v).await.unwrap();
        assert_eq!(e.nr_date, NaiveDate::from_ymd_opt(2023, 5, 25).unwrap());
        assert_eq!(
            e.br_date,
            if br {
                NaiveDate::from_ymd_opt(2023, 6, 7)
            } else {
                None
            }
        );
        assert_eq!(e.legislative_initiative.id, "XXVII/A/3373");
    }
}

#[test]
fn delegate_mandates_preserve_dates_party_and_function_flags() {
    use somes_scraper::delegates::extract_mandate;
    for (function, name, nr, gov, ministry, chancellor) in [
        (
            "NR",
            "Abgeordnete zum Nationalrat",
            true,
            false,
            false,
            false,
        ),
        ("BK", "Bundeskanzlerin", false, true, false, true),
        ("BM", "Bundesministerin", false, true, true, false),
        ("STS", "Staatssekretärin", false, true, false, false),
    ] {
        let v = json!({"mandat":name,"mandatVon":"01.01.2020","mandatBis":"","wahlpartei":"TEST","wahlkreis":"Wien","funktion":function});
        let m = extract_mandate(&v).unwrap();
        assert_eq!(
            (m.is_nr, m.is_gov, m.is_ministry, m.is_chancellor),
            (nr, gov, ministry, chancellor)
        );
        assert_eq!(m.party.as_deref(), Some("TEST"));
        assert_eq!(m.constituency.as_deref(), Some("Wien"));
        assert_eq!(m.start, NaiveDate::from_ymd_opt(2020, 1, 1).unwrap());
        assert_eq!(m.end, None);
        let mut ended = v.clone();
        ended["mandatBis"] = json!("31.12.2021");
        assert_eq!(
            extract_mandate(&ended).unwrap().end,
            NaiveDate::from_ymd_opt(2021, 12, 31)
        );
        let mut bad = v;
        bad["mandatVon"] = json!("31.02.2020");
        assert!(extract_mandate(&bad).is_none());
    }
}

#[test]
fn legislative_dates_and_majority_handle_missing_stages_and_fallbacks() {
    use somes_scraper::legis_init::{extract_majority_on_indicators, extract_plenary_date};
    assert_eq!(
        extract_plenary_date(
            &json!({"stages":[{"priority":1},{"priority":1,"date":"01.02.2020"}]}),
            "NR"
        ),
        Some("01.02.2020")
    );
    assert_eq!(extract_plenary_date(&json!({}), "NR"), None);
    assert_eq!(
        extract_plenary_date(&json!({"phase":[{"name":"NR"}]}), "NR"),
        None
    );
    for (description, simple) in [
        ("angenommen", true),
        ("nicht die erforderliche 2/3 Mehrheit", false),
    ] {
        assert_eq!(
            extract_majority_on_indicators(
                &json!({"phase":[{}, {"stages":[]}],"status":{"description":description}})
            ),
            Some(simple)
        );
    }
}

#[test]
fn documents_reject_incomplete_entries_and_preserve_unknown_types() {
    use somes_scraper::documents;
    assert_eq!(documents(&[], "https://example.org"), Some(vec![]));
    assert!(documents(
        &[json!({"documents":[{"type":"PDF"}]})],
        "https://example.org"
    )
    .is_none());
    let d = documents(
        &[json!({"title":" Anhang ","documents":[{"type":"DOCX","link":"/a.docx"}]})],
        "https://example.org",
    )
    .unwrap();
    assert_eq!(d.len(), 1);
    assert_eq!(d[0].title, "Anhang");
    assert_eq!(d[0].link, "https://example.org/a.docx");
    assert_eq!(d[0].ftype, FileType::Unknown);
}

#[test]
fn html_name_extraction_preserves_entities_and_stops_before_next_speaker() {
    let html = "<section><p><a name=\"RU_1\"></a>Erika: Grüß &amp; Gott.</p></section><section><p><a name=\"RU_2\"></a>Zweite Rede.</p></section>";
    let document = scraper::Html::parse_document(html);
    assert_eq!(
        somes_scraper::extract_text_from_html_till_next_name_attribute(html, "RU_1"),
        "Erika: Grüß & Gott."
    );
    assert_eq!(
        somes_scraper::extract_text_from_html_via_name_attribute_parent_parent(&document, "RU_1"),
        "Erika: Grüß & Gott."
    );
    assert_eq!(
        somes_scraper::extract_text_from_html_via_name_attribute_finding_optimal_solution(
            &document,
            "RU_1",
            TextExtractor::default()
        )
        .as_deref(),
        Some("Erika: Grüß & Gott.")
    );
    let document = scraper::Html::parse_document("<a name=\"T_1\"><b>Grüß</b> &amp; Gott</a>");
    assert_eq!(
        somes_scraper::extract_text_from_html_via_name_attribute(&document, "T_1"),
        "Grüß & Gott"
    );
}

#[test]
fn html_missing_speech_anchor_returns_empty_text() {
    let html = "<p>Andere Rede.</p>";
    let document = scraper::Html::parse_document(html);
    assert_eq!(
        somes_scraper::extract_text_from_html_via_name_attribute(&document, "missing"),
        ""
    );
    assert_eq!(
        somes_scraper::extract_text_from_html_via_name_attribute_parent_parent(
            &document, "missing"
        ),
        ""
    );
    assert_eq!(
        somes_scraper::extract_text_from_html_till_next_name_attribute(html, "missing"),
        ""
    );
}

#[test]
fn html_randnummer_extraction_tolerates_whitespace_and_multiple_css_classes() {
    let html="<p id=\"1\" class=\"randnummer\">1</p>\n<!-- paragraph separator -->\n<p>Erster Absatz.</p>\n<p>Zweiter Absatz &amp; Ende.</p>\n<p id=\"2\" class=\"marker randnummer\">2</p><p>Nächste Rede.</p>";
    let document = scraper::Html::parse_document(html);
    assert_eq!(
        somes_scraper::extract_text_from_html_via_rn_id(&document, "1"),
        "Erster Absatz.\nZweiter Absatz & Ende.\n"
    );
    assert_eq!(
        somes_scraper::extract_text_from_html_via_rn_id(&document, "missing"),
        ""
    );
}

#[test]
fn speech_fit_stops_before_president_and_normalizes_control_characters() {
    let extractor = TextExtractor::default();
    let text="Erika Beispiel: Grüß\u{a0}Gott!\u{ad}\r\n\0Präsident: Als Nächste spricht Frau Muster.\r\nWeitere Rede.";
    assert_eq!(
        somes_scraper::fit_speech(
            text.into(),
            "Erika Beispiel",
            &extractor.fit_speech_regexes,
            true
        )
        .unwrap(),
        "Erika Beispiel: Grüß Gott!"
    );
}
