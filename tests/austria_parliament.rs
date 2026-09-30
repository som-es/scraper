//! Regression tests against checked-in responses from parlament.gv.at.
//! Call parsers directly: these fixtures need no HTTP requests or disk cache.
//! The legislative fixtures contain party votes, not roll-call votes that
//! would require fetching additional stenographic documents.

use chrono::NaiveDate;
use serde_json::{json, Value};
use somes_scraper::{
    legis_init::LegislativeInitiatives, proposals::ProposalScraper, AcceptedState, AustriaScraper,
    Document, FileType, LegislativeInitiative, Proposal, Voting,
};

const ADOPTED: &str = include_str!("../test-data/gegenstand/XXVII/A/3373.json");
const PENDING: &str = include_str!("../test-data/gegenstand/XXVIII/A/104.json");

fn raw_fixture(source: &str) -> Value {
    serde_json::from_str(source).expect("fixture must contain valid JSON")
}

fn proposal(source: &str) -> Proposal {
    AustriaScraper::raw_proposal_into_proposal(raw_fixture(source))
        .expect("parliament fixture must parse as a proposal")
}

async fn initiative(source: &str) -> LegislativeInitiative {
    AustriaScraper::default()
        .raw_legis_into_legis(raw_fixture(source))
        .await
        .expect("parliament fixture must parse as a legislative initiative")
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

#[test]
fn proposal_preserves_identity_authors_and_topics() {
    let proposal = proposal(ADOPTED);

    assert_eq!(proposal.id, "A/3373");
    assert_eq!(proposal.gp, "XXVII");
    assert_eq!(proposal.ityp, "A");
    assert_eq!(proposal.inr, 3373);
    assert_eq!(proposal.title, "Erdgasabgabegesetz, Elektrizitätsabgabegesetz u.a., Änderung (3373/A) | Parlament Österreich");
    assert_eq!(proposal.issuer_ids, [83148, 5677]);
    assert!(proposal.receiver_ids.is_empty());
    assert_eq!(
        proposal.topics,
        ["Budget und Finanzen", "Klima, Umwelt und Energie"]
    );
    assert_eq!(
        proposal.eurovoc_topics,
        ["Elektrizitätsindustrie", "Energie", "Steuerwesen"]
    );
    assert_eq!(
        proposal.other_keyword_topics,
        ["Energiewirtschaft", "Elektrizität", "Steuern und Gebühren"]
    );
}

#[test]
fn proposal_normalizes_all_document_links_and_file_types() {
    let proposal = proposal(PENDING);
    let expected = [
        (
            "Selbständiger Antrag (gescanntes Original)",
            "imfname_1671948.pdf",
            FileType::OriginalPDF,
        ),
        (
            "Initiativantrag (elektr. übermittelte Version)",
            "fname_1672027.pdf",
            FileType::DigitalPDF,
        ),
        (
            "Initiativantrag (elektr. übermittelte Version)",
            "fnameorig_1672027.html",
            FileType::HTML,
        ),
        (
            "Gesetzestext (Arbeitsdokument ParlDion)",
            "fname_1672117.pdf",
            FileType::DigitalPDF,
        ),
        (
            "Gesetzestext (Arbeitsdokument ParlDion)",
            "fnameorig_1672117.html",
            FileType::HTML,
        ),
        (
            "Textgegenüberstellung (Arbeitsdokument ParlDion)",
            "fname_1672377.pdf",
            FileType::DigitalPDF,
        ),
        (
            "Textgegenüberstellung (Arbeitsdokument ParlDion)",
            "fnameorig_1672377.html",
            FileType::HTML,
        ),
    ]
    .map(|(title, filename, ftype)| Document {
        title: title.into(),
        link: format!("https://www.parlament.gv.at/dokument/XXVIII/A/104/{filename}"),
        ftype,
    });

    assert_eq!(proposal.documents, expected);
}

#[test]
fn proposal_converts_vienna_summer_and_winter_times_to_utc() {
    for (source, created, updated) in [
        (
            ADOPTED,
            "2023-05-11T22:00:00+00:00",
            "2025-12-25T06:57:12+00:00",
        ),
        (
            PENDING,
            "2025-03-06T23:00:00+00:00",
            "2026-06-23T23:50:09+00:00",
        ),
    ] {
        let proposal = proposal(source);
        assert_eq!(proposal.created_at.to_rfc3339(), created);
        assert_eq!(proposal.updated_at.unwrap().to_rfc3339(), updated);
    }
}

#[test]
fn proposal_json_preserves_field_names_types_and_nulls() {
    let mut raw = raw_fixture(PENDING);
    // Exercise absent optional content independently of the complete fixture.
    for field in [
        "description",
        "names",
        "documents",
        "topics",
        "eurovoc",
        "headwords",
    ] {
        raw["content"].as_object_mut().unwrap().remove(field);
    }
    let proposal = AustriaScraper::raw_proposal_into_proposal(raw).unwrap();

    assert_eq!(
        serde_json::to_value(proposal).unwrap(),
        json!({
            "id": "A/104",
            "ityp": "A",
            "gp": "XXVIII",
            "inr": 104,
            "title": "Beamten-Kranken- und Unfallversicherungsgesetz, Änderung (104/A) | Parlament Österreich",
            "proposal_type": {"Other": "A"},
            "description": null,
            "issuer_ids": [],
            "receiver_ids": [],
            "documents": [],
            "topics": [],
            "eurovoc_topics": [],
            "other_keyword_topics": [],
            "created_at": "2025-03-06T23:00:00Z",
            "updated_at": "2026-06-23T23:50:09Z"
        })
    );
}

#[test]
fn proposal_rejects_missing_required_fields() {
    for pointer in [
        "/meta/title",
        "/content/ityp",
        "/content/inr",
        "/content/gp_code",
        "/content/einlangen",
    ] {
        let mut raw = raw_fixture(PENDING);
        *raw.pointer_mut(pointer).unwrap() = Value::Null;
        assert!(
            AustriaScraper::raw_proposal_into_proposal(raw).is_none(),
            "accepted missing field {pointer}"
        );
    }
}

#[test]
fn proposal_rejects_invalid_numbers_and_creation_dates() {
    for (pointer, invalid) in [
        ("/content/inr", json!("104")),
        ("/content/inr", json!(-1)),
        ("/content/einlangen", json!("2025-02-30T00:00:00")),
        ("/content/einlangen", json!("07.03.2025")),
    ] {
        let mut raw = raw_fixture(PENDING);
        *raw.pointer_mut(pointer).unwrap() = invalid.clone();
        assert!(
            AustriaScraper::raw_proposal_into_proposal(raw).is_none(),
            "accepted {pointer} = {invalid}"
        );
    }
}

#[tokio::test]
async fn adopted_initiative_preserves_votes_and_dates_for_both_councils() {
    let init = initiative(ADOPTED).await;

    assert_eq!(init.id, "XXVII/A/3373");
    assert_eq!(
        init.description,
        "Erdgasabgabegesetz, Elektrizitätsabgabegesetz u.a., Änderung (3373/A)"
    );
    assert_eq!(init.accepted, Some(AcceptedState::Accepted));
    assert_eq!(init.law_accepted, Some(true));
    assert_eq!(init.voting, Some(Voting::Law));
    assert!(init.is_law);
    assert!(init.is_voteable_on);
    assert!(init.requires_simple_majority);
    assert_eq!(init.nr_plenary_activity_date, date(2023, 5, 25));
    assert_eq!(init.br_plenary_activity_date, Some(date(2023, 6, 7)));
    assert_eq!(init.vote_date, Some(date(2023, 5, 25)));
    assert_eq!(init.plenary_session, Some(215));
    assert_eq!(init.br_plenary_session, Some(954));

    let votes: Vec<_> = init
        .votes
        .iter()
        .map(|vote| (vote.party.code.as_str(), vote.party.fraction, vote.infavor))
        .collect();
    assert_eq!(
        votes,
        [
            ("V", 71, true),
            ("S", 40, false),
            ("F", 30, true),
            ("G", 26, true),
            ("N", 15, false)
        ]
    );

    let nr = init.hidden_votes_info.pro_contra_parties.unwrap();
    assert_eq!(nr.pro, ["ÖVP", "FPÖ", "GRÜNE"]);
    assert_eq!(nr.contra, ["SPÖ", "NEOS"]);
    let br = init
        .br_hidden_votes_info
        .unwrap()
        .pro_contra_parties
        .unwrap();
    assert_eq!(br.pro, ["ÖVP", "FPÖ", "GRÜNE"]);
    assert_eq!(br.contra, ["SPÖ", "OF"]);
}

#[tokio::test]
async fn adopted_initiative_uses_final_law_documents() {
    let init = initiative(ADOPTED).await;
    let law_documents: Vec<_> = init
        .documents
        .iter()
        .filter(|document| document.title.contains("Gesetzestext"))
        .collect();

    assert_eq!(
        law_documents,
        [
            &Document {
                title: "Gesetzestext".into(),
                link: "https://www.parlament.gv.at/dokument/XXVII/BNR/743/fname_1566865.pdf".into(),
                ftype: FileType::DigitalPDF
            },
            &Document {
                title: "Gesetzestext".into(),
                link: "https://www.parlament.gv.at/dokument/XXVII/BNR/743/fnameorig_1566865.html"
                    .into(),
                ftype: FileType::HTML
            },
        ]
    );
    assert_eq!(
        init.ris_link.as_deref(),
        Some(
            "http://www.ris.bka.gv.at/Dokument.wxe?Abfrage=BgblAuth&Dokumentnummer=BGBLA_2023_I_64"
        )
    );
}

#[tokio::test]
async fn pending_initiative_does_not_invent_a_vote_result() {
    let init = initiative(PENDING).await;

    assert_eq!(init.id, "XXVIII/A/104");
    assert_eq!(init.status, Some(2));
    assert!(init.is_law);
    assert!(init.is_voteable_on);
    assert_eq!(init.accepted, None);
    assert_eq!(init.law_accepted, None);
    assert_eq!(init.vote_date, None);
    assert_eq!(init.plenary_session, None);
    assert_eq!(init.br_plenary_session, None);
    assert_eq!(init.br_plenary_activity_date, None);
    assert_eq!(init.ris_link, None);
    assert!(init.votes.is_empty());
    assert_eq!(init.proposal.issuer_ids, [5670]);
}

#[tokio::test]
async fn initiative_json_preserves_dates_enums_and_numeric_ids() {
    let value = serde_json::to_value(initiative(ADOPTED).await).unwrap();

    assert_eq!(value["id"], json!("XXVII/A/3373"));
    assert_eq!(value["inr"], json!(3373));
    assert_eq!(value["accepted"], json!("Accepted"));
    assert_eq!(value["voting"], json!("Law"));
    assert_eq!(value["nr_plenary_activity_date"], json!("2023-05-25"));
    assert_eq!(value["br_plenary_activity_date"], json!("2023-06-07"));
    assert_eq!(value["created_at"], json!("2023-05-11T22:00:00Z"));
    assert_eq!(
        value["related_delegates"],
        json!([
            {"delegate_id": 83148, "text": "Eingebracht von"},
            {"delegate_id": 5677, "text": "Eingebracht von"}
        ])
    );
    assert_eq!(
        value["documents"][0],
        json!({
            "title": "Selbständiger Antrag (gescanntes Original)",
            "link": "https://www.parlament.gv.at/dokument/XXVII/A/3373/imfname_1559534.pdf",
            "ftype": "OriginalPDF"
        })
    );
}

#[test]
fn initiative_paths_normalize_numeric_and_zero_padded_ids() {
    let scraper = AustriaScraper::default();
    let paths = scraper
        .legis_paths(json!({"rows": [
            ["XXVII", "A", "03373"],
            ["XXVIII", "A", 104],
            ["XXVII", "I", "01774"]
        ]}))
        .unwrap();

    assert_eq!(
        paths,
        [
            "/gegenstand/XXVII/A/3373",
            "/gegenstand/XXVIII/A/104",
            "/gegenstand/XXVII/I/1774"
        ]
    );
    assert!(scraper.legis_paths(json!({"rows": null})).is_err());
}

#[tokio::test]
async fn pending_initiative_serializes_unknown_results_as_null() {
    let value = serde_json::to_value(initiative(PENDING).await).unwrap();

    for field in [
        "accepted",
        "law_accepted",
        "vote_date",
        "plenary_session",
        "br_plenary_session",
        "br_plenary_activity_date",
        "ris_link",
    ] {
        assert_eq!(
            value.get(field),
            Some(&Value::Null),
            "{field} must be present as null"
        );
    }
    assert_eq!(value["votes"], json!([]));
    assert_eq!(value["is_law"], json!(true));
    assert_eq!(value["is_voteable_on"], json!(true));
}

#[test]
fn plenary_text_preserves_the_complete_closing_statement_without_html() {
    let html = scraper::Html::parse_document(include_str!(
        "../test-data/www.parlament.gv.at/dokument/XXVIII/NRSITZ/6/A_-_16_39_34_00598770.html.json"
    ));
    let text = somes_scraper::extract_text_from_html_via_rn_id(&html, "2.4");

    assert_eq!(text.trim(), concat!(
        "Bevor ich die Sitzung formal beende, darf ich allen Abgeordneten, aber auch den Mitarbeiterinnen und Mitarbeitern des Hauses eine besinnliche Adventzeit und eine erholsame, geruhsame und besinnliche Weihnachtszeit wünschen. ",
        "Das Erholsame gilt vor allem für diejenigen, die sich auch im Interesse der Republik um eine neue Bundesregierung bemühen müssen, dass doch wenigstens ein paar Minuten für Besinnung und Zeit mit der Familie bleiben.\n\n",
        "Die Sitzung ist geschlossen."
    ));
}
