use chrono::NaiveDate;
use common_scrapes::{FileType, Reference, StatementAuthor, StatementAuthorKind};

use crate::{
    statements::{
        StatementItyp, StatementListItem, StatementRowIndices, StatementsPayload,
        StatementsScraper,
    },
    AustriaScraper,
};

fn reference(gp: &str, ityp: &str, inr: u32) -> Reference {
    Reference {
        gp: gp.to_string(),
        ityp: ityp.to_string(),
        inr,
    }
}

fn statement_item(gp: &str, ityp: &str, inr: u64, kind: StatementAuthorKind) -> StatementListItem {
    StatementListItem::new(gp, ityp, inr, kind)
}

#[test]
fn test_statements_payload_json() {
    let payload = StatementsPayload {
        reference: Some(reference("XXVIII", "ME", 126)),
        ityp: vec![StatementItyp::Snme],
        ..Default::default()
    };

    assert_eq!(
        payload.as_json(),
        serde_json::json!({
            "VHG": ["1"],
            "ITYP": ["SNME"],
            "BEZUG_GP_CODE": ["XXVIII"],
            "BEZUG_ITYP": ["ME"],
            "BEZUG_INR": [126]
        })
    );
}

#[test]
fn test_statements_payload_gp_and_datum() {
    let payload = StatementsPayload::new().with_datum(vec![
        NaiveDate::from_ymd_opt(2025, 9, 1).unwrap(),
        NaiveDate::from_ymd_opt(2026, 7, 6).unwrap(),
    ]);

    assert_eq!(
        payload.as_json(),
        serde_json::json!({
            "VHG": ["1"],
            "GP_CODE": ["XXVIII"],
            "DATUM": ["01.09.2025", "06.07.2026"]
        })
    );
}

#[test]
fn test_statement_ityp_reference_mapping() {
    assert_eq!(
        StatementItyp::from_reference_ityp("ME"),
        Some(StatementItyp::Snme)
    );
    assert_eq!(StatementItyp::Snme.reference_ityp(), "ME");
    assert_eq!(StatementItyp::Sbi.reference_ityp(), "BI");
    assert_eq!(StatementItyp::Spet.reference_ityp(), "PET");
    assert_eq!(StatementItyp::SpetBr.as_str(), "SPET-BR");
    assert_eq!(
        StatementItyp::from_reference_ityp("AUA"),
        Some(StatementItyp::Sn)
    );
    assert_eq!(StatementItyp::from_ityp("SNME"), Some(StatementItyp::Snme));
    assert_eq!(
        StatementsPayload::for_reference(reference("XXVIII", "ME", 126)).ityp,
        vec![StatementItyp::Snme]
    );
}

#[tokio::test]
async fn test_statement_paths_row_columns_match_header() {
    let scraper = AustriaScraper::default();
    let statements = scraper
        .post_statements_payload(StatementsPayload::new().with_gp("XXVIII"))
        .await
        .unwrap();

    let indices = StatementRowIndices::from_header(&statements["header"]);
    assert_eq!(indices, StatementRowIndices::default());

    let paths = scraper.statement_paths(statements).unwrap();
    assert!(paths.len() > 100);
    assert!(paths.iter().all(|path| path.inr > 0));
    assert!(paths.iter().all(|path| path.datum.is_some()));
    assert!(paths.iter().all(|path| path.reference.is_some()));
    assert!(paths
        .iter()
        .any(|path| path.author_kind == StatementAuthorKind::Organisation));
    assert!(paths
        .iter()
        .any(|path| path.author_kind == StatementAuthorKind::PrivatePerson));
}

#[tokio::test]
async fn test_statement_paths_for_reference_me_126() {
    let scraper = AustriaScraper::default();
    let me_126 = reference("XXVIII", "ME", 126);

    let paths = scraper
        .statement_paths_of_payload(StatementsPayload::for_reference(me_126.clone()))
        .await
        .unwrap();

    assert!(paths.len() > 100);
    assert!(paths.iter().all(|path| path.reference == Some(me_126.clone())));
    assert!(paths
        .iter()
        .all(|path| path.path.starts_with("/gegenstand/XXVIII/SNME/")));
    assert!(paths
        .iter()
        .any(|path| path.author_kind == StatementAuthorKind::Organisation));
    assert!(paths
        .iter()
        .any(|path| path.author_kind == StatementAuthorKind::PrivatePerson));

    let statement = scraper.get_statement(paths[0].clone()).await.unwrap();
    assert_eq!(statement.primary_reference, Some(me_126));
    assert_eq!(statement.ityp, "SNME");
}

#[tokio::test]
async fn test_statement_paths_skips_unpublished() {
    let scraper = AustriaScraper::default();

    let paths = scraper
        .statement_paths_of_payload(
            StatementsPayload::new()
                .with_gp("XXVIII")
                .with_ityps(vec![StatementItyp::Sn]),
        )
        .await
        .unwrap();

    assert!(paths.len() > 100);
    assert!(!paths
        .iter()
        .any(|path| path.author_name == "Nicht-öffentliche Stellungnahme"));
    assert!(!paths.iter().any(|path| path.path == "/gegenstand/XXVIII/SN/386"));
}

#[tokio::test]
async fn test_statements_paging() {
    let scraper = AustriaScraper::default();
    let payload = StatementsPayload::new()
        .with_gp("XXVIII")
        .with_ityps(vec![StatementItyp::Spet]);

    let single = scraper
        .statement_paths_of_payload(payload.clone())
        .await
        .unwrap();
    let paged = scraper
        .paged_statement_paths(&payload, 100)
        .await
        .unwrap();

    assert!(single.len() > 300);
    assert_eq!(paged.len(), single.len());

    let paths = |mut items: Vec<StatementListItem>| {
        items.sort_by(|left, right| left.path.cmp(&right.path));
        items
            .into_iter()
            .map(|item| item.path)
            .collect::<Vec<String>>()
    };

    let paged_paths = paths(paged);
    let single_paths = paths(single);

    assert_eq!(paged_paths, single_paths);

    let mut unique_paths = paged_paths.clone();
    unique_paths.dedup();
    assert_eq!(unique_paths.len(), paged_paths.len());
}

#[tokio::test]
async fn test_get_snme_statement_of_private_person() {
    let scraper = AustriaScraper::default();

    let statement = scraper
        .get_statement(statement_item(
            "XXVIII",
            "SNME",
            4729,
            StatementAuthorKind::PrivatePerson,
        ))
        .await
        .unwrap();

    assert_eq!(statement.id, 4729);
    assert_eq!(statement.ityp, "SNME");
    assert_eq!(statement.zitation, "78/SN-126/ME");
    assert_eq!(
        statement.title,
        "Bundesgesetz zur Einführung einer Bundesstaatsanwaltschaft"
    );
    assert!(!statement.organisation());
    assert_eq!(
        statement.authors,
        vec![StatementAuthor {
            name: "Haslinger, Christian (7012 Zagersdorf)".into(),
            delegate_id: None,
            party_code: None,
            institution: None,
        }]
    );
    assert!(statement.documents.is_empty());
    assert!(statement
        .statement_text
        .unwrap()
        .contains("unabhängige Bundesstaatsanwaltschaft"));
    assert_eq!(statement.references, vec![reference("XXVIII", "ME", 126)]);
    assert_eq!(
        statement.primary_reference,
        Some(reference("XXVIII", "ME", 126))
    );
    assert!(statement.topics.contains(&"Inneres und Recht".to_string()));
    assert!(statement
        .eurovoc_topics
        .contains(&"Gerichtswesen".to_string()));
    assert!(statement
        .other_keyword_topics
        .contains(&"Bundesverfassung".to_string()));
    assert_eq!(statement.stage, Some("Einlangen".into()));
    assert_eq!(statement.created_at.to_rfc3339(), "2026-07-05T22:00:00+00:00");
    assert_eq!(
        statement.updated_at.map(|update| update.to_rfc3339()),
        Some("2026-07-06T15:49:49+00:00".to_string())
    );
    assert_eq!(statement.path, "/gegenstand/XXVIII/SNME/4729");
}

#[tokio::test]
async fn test_get_snme_statement_of_organisation() {
    let scraper = AustriaScraper::default();

    let mut item = statement_item(
        "XXVIII",
        "SNME",
        1510,
        StatementAuthorKind::Organisation,
    );
    item.supports = 3;
    let statement = scraper.get_statement(item).await.unwrap();

    assert_eq!(statement.zitation, "14/SN-46/ME");
    assert!(statement.organisation());
    assert_eq!(
        statement.authors[0].name,
        "Amt der Vorarlberger Landesregierung"
    );
    assert_eq!(statement.supports, 3);
    assert_eq!(statement.statement_text, None);
    assert_eq!(statement.documents[0].title, "Stellungnahme");
    assert_eq!(statement.documents[0].ftype, FileType::DigitalPDF);
    assert_eq!(
        statement.documents[0].link,
        "https://www.parlament.gv.at/PtWeb/api/s3serv/file/304bc548-271e-41db-9653-465a4681ca99"
    );
    assert_eq!(
        statement.primary_reference,
        Some(reference("XXVIII", "ME", 46))
    );
}

#[tokio::test]
async fn test_get_sn_statement_of_ministry() {
    let scraper = AustriaScraper::default();

    let statement = scraper
        .get_statement(statement_item(
            "XXVIII",
            "SN",
            552,
            StatementAuthorKind::Organisation,
        ))
        .await
        .unwrap();

    assert_eq!(statement.zitation, "552/SN");
    assert!(statement.organisation());
    assert_eq!(
        statement.authors,
        vec![StatementAuthor {
            name: "Andreas Babler, MSc".into(),
            delegate_id: Some(23963),
            party_code: Some("S".into()),
            institution: Some(
                "Bundesministerium für Wohnen, Kunst, Kultur, Medien und Sport".into()
            ),
        }]
    );
    assert_eq!(statement.statement_text, None);
    assert_eq!(statement.documents.len(), 1);
    assert_eq!(
        statement.documents[0].link,
        "https://www.parlament.gv.at/dokument/XXVIII/SN/552/imfname_1709923.pdf"
    );
    assert_eq!(
        statement.references,
        vec![
            reference("XXVIII", "AUA", 33),
            reference("XXVIII", "A", 372)
        ]
    );
    assert_eq!(
        statement.primary_reference,
        Some(reference("XXVIII", "A", 372))
    );
}

#[tokio::test]
async fn test_get_spet_statement_with_documents_and_text() {
    let scraper = AustriaScraper::default();

    let statement = scraper
        .get_statement(statement_item(
            "XXVIII",
            "SPET",
            8,
            StatementAuthorKind::PrivatePerson,
        ))
        .await
        .unwrap();

    assert_eq!(statement.zitation, "8/SPET");
    assert!(statement.statement_text.is_some());
    assert_eq!(statement.documents.len(), 1);
    assert_eq!(statement.documents[0].ftype, FileType::DigitalPDF);
    assert_eq!(statement.stage, Some("Einlangen im Nationalrat".into()));
    assert_eq!(
        statement.primary_reference,
        Some(reference("XXVIII", "PET", 3))
    );
}

#[tokio::test]
async fn test_get_spet_statement_of_ministry() {
    let scraper = AustriaScraper::default();

    let statement = scraper
        .get_statement(statement_item(
            "XXVII",
            "SPET",
            2795,
            StatementAuthorKind::Organisation,
        ))
        .await
        .unwrap();

    assert_eq!(statement.zitation, "2795/SPET");
    assert_eq!(statement.authors[0].delegate_id, Some(5653));
    assert_eq!(statement.authors[0].party_code, Some("G".into()));
    assert_eq!(
        statement.authors[0].institution,
        Some("Bundesministerium für Klimaschutz, Umwelt, Energie, Mobilität, Innovation und Technologie".into())
    );
    assert_eq!(
        statement.references,
        vec![
            reference("XXVII", "AUA", 374),
            reference("XXVII", "PET", 126)
        ]
    );
    assert_eq!(
        statement.primary_reference,
        Some(reference("XXVII", "PET", 126))
    );
}

#[tokio::test]
async fn test_get_sbi_statement() {
    let scraper = AustriaScraper::default();

    let statement = scraper
        .get_statement(statement_item(
            "XXVII",
            "SBI",
            5090,
            StatementAuthorKind::PrivatePerson,
        ))
        .await
        .unwrap();

    assert_eq!(statement.gp, "XXVII");
    assert_eq!(statement.zitation, "5090/SBI");
    assert_eq!(statement.title, "Stellungnahme zu 58/BI");
    assert_eq!(statement.authors[0].name, "Hafezi Birkani, Amin (1100 Wien)");
    assert_eq!(statement.references, vec![reference("XXVII", "BI", 58)]);
    assert_eq!(
        statement.updated_at.map(|update| update.date_naive()),
        NaiveDate::from_ymd_opt(2024, 3, 24)
    );
}

#[tokio::test]
async fn test_statements_of_reference() {
    let scraper = AustriaScraper::default();

    let mut paths = scraper
        .statement_paths_of_payload(StatementsPayload::for_reference(reference(
            "XXVIII", "ME", 126,
        )))
        .await
        .unwrap();
    paths.truncate(3);

    let mut statements = Vec::new();
    for path in paths {
        statements.push(scraper.get_statement(path).await.unwrap());
    }

    assert_eq!(statements.len(), 3);
    assert!(statements.iter().all(|statement| statement.ityp == "SNME"));
    assert!(statements
        .iter()
        .all(|statement| statement.primary_reference == Some(reference("XXVIII", "ME", 126))));
    assert!(statements
        .iter()
        .all(|statement| statement.path.starts_with("/gegenstand/XXVIII/SNME/")));
}
