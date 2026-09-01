use chrono::NaiveDate;
use common_scrapes::{FileType, Volksbg, VolksbgState};

use super::*;

const ANGEKUENDIGT: &str = "2026-05-08_wayback__bmi411__eintragungszeitraum_angekuendigt.html";
const LAUFEND: &str = "2026-06-14_wayback__bmi411__eintragungszeitraum_laufend.html";
const LAUFEND_SPAETER_TAG: &str = "2026-06-18_wayback__bmi411__eintragungszeitraum_laufend.html";
const ERGEBNISSE: &str = "2026-08-20_wayback__bmi411__ergebnisse_festgestellt.html";
const UNTERSTUETZUNG: &str = "2026-09-01__bmi411__unterstuetzung_und_ergebnisse.html";
const DETAIL: &str = "2026-09-01__bmi411__detail_gratis_verhuetung.html";
const DETAIL_NACH_STATTGEBUNG: &str =
    "2026-02-16_wayback__bmi411__detail_gratis_verhuetung_nach_stattgebung.html";
const GP_INDEX: &str = "2026-09-01__bmi411__gp_index.html";
const DETAIL_ALTE_RS: &str = "2025-11-16_wayback__bmi411__detail_arbeitslosengeld_rauf.html";
const DETAIL_ANTI_GENDERN: &str = "2026-04-05_wayback__bmi411__detail_anti_gendern.html";
const DETAIL_ASYL: &str = "2026-03-11_wayback__bmi411__detail_asyl_europagerecht.html";

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/test-data/bmi/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .expect("fixture should exist")
}

fn date(year: i32, month: u32, day: u32) -> NaiveDate {
    NaiveDate::from_ymd_opt(year, month, day).unwrap()
}

fn detail_page(slug: &str) -> String {
    format!("{BMI_BASE_URL}/411/volksbegehren_der_xx_gesetzgebungsperiode/{slug}/start.html")
}

#[test]
fn eintragungswoche_wird_angekuendigt() {
    let woche = parse_eintragungswoche(&fixture(ANGEKUENDIGT), BMI_VOLKSBEGEHREN_URL).unwrap();

    assert_eq!(woche.start, date(2026, 6, 15));
    assert_eq!(woche.end, date(2026, 6, 22));
    assert_eq!(woche.polling_stations_url, None);

    assert_eq!(
        woche
            .volksbegehren
            .iter()
            .map(|volksbg| volksbg.id.as_str())
            .collect::<Vec<_>>(),
        [
            "karfreitag_feiertag_fuer_alle",
            "polizei_-_kritischer_personalmangel",
            "wahlpflicht_nationalratswahl_bundespraesidentenwahl",
            "gratis_verhuetung",
            "transparenz_im_parlament"
        ]
    );

    assert_eq!(woche.volksbegehren[0].title, "Karfreitag-Feiertag für Alle");
    assert_eq!(
        woche.volksbegehren[3].overview_url,
        "https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html"
    );
}

#[test]
fn alle_volksbegehren_einer_woche_haben_eine_id() {
    let woche = parse_eintragungswoche(&fixture(ANGEKUENDIGT), BMI_VOLKSBEGEHREN_URL).unwrap();

    assert!(woche
        .volksbegehren
        .iter()
        .all(|volksbg| !volksbg.id.is_empty() && !volksbg.title.is_empty()));
}

#[test]
fn zustaende_einer_eintragungswoche() {
    let woche = parse_eintragungswoche(&fixture(ANGEKUENDIGT), BMI_VOLKSBEGEHREN_URL).unwrap();

    assert_eq!(woche.state_on(date(2026, 5, 8)), VolksbgState::Scheduled);
    assert_eq!(woche.state_on(date(2026, 6, 15)), VolksbgState::Open);
    assert_eq!(woche.state_on(date(2026, 6, 22)), VolksbgState::Open);

    assert!(!woche.is_open_on(date(2026, 6, 14)));
    assert!(woche.is_open_on(date(2026, 6, 15)));
    assert!(!woche.is_open_on(date(2026, 6, 23)));

    let mut woche = woche;
    woche.apply_state(date(2026, 6, 16));

    assert!(woche
        .volksbegehren
        .iter()
        .all(|volksbg| volksbg.state == Some(VolksbgState::Open)));
}

#[test]
fn laufende_eintragungswoche_liefert_stichtag_und_lokale() {
    let woche = parse_eintragungswoche(&fixture(LAUFEND), BMI_VOLKSBEGEHREN_URL).unwrap();

    assert_eq!(woche.start, date(2026, 6, 15));
    assert_eq!(woche.end, date(2026, 6, 22));
    assert_eq!(woche.cut_off_date, Some(date(2026, 5, 11)));
    assert_eq!(
        woche.online_deadline_utc.unwrap().to_rfc3339(),
        "2026-06-22T18:00:00+00:00"
    );

    let polling_stations_url = woche.polling_stations_url.unwrap();
    assert!(
        polling_stations_url.ends_with(".xlsx"),
        "{polling_stations_url}"
    );
    assert!(polling_stations_url.starts_with("https://www.bmi.gv.at/411/"));

    assert_eq!(woche.volksbegehren.len(), 5);
    assert_eq!(woche.volksbegehren[4].id, "transparenz_im_parlament");
}

#[test]
fn laufende_eintragungswoche_am_zweiten_tag() {
    let woche =
        parse_eintragungswoche(&fixture(LAUFEND_SPAETER_TAG), BMI_VOLKSBEGEHREN_URL).unwrap();

    assert_eq!(woche.start, date(2026, 6, 15));
    assert_eq!(woche.end, date(2026, 6, 22));
    assert_eq!(woche.volksbegehren.len(), 5);
    assert_eq!(
        woche
            .volksbegehren
            .iter()
            .map(|volksbg| volksbg.title.as_str())
            .collect::<Vec<_>>(),
        [
            "Karfreitag-Feiertag für Alle",
            "Polizei - kritischer Personalmangel",
            "Wahlpflicht Nationalratswahl Bundespräsidentenwahl",
            "GRATIS Verhütung",
            "Transparenz im Parlament"
        ]
    );
}

#[test]
fn keine_eintragungswoche_wenn_ergebnisse_feststehen() {
    assert_eq!(
        parse_eintragungswoche(&fixture(ERGEBNISSE), BMI_VOLKSBEGEHREN_URL),
        None
    );
    assert_eq!(
        parse_eintragungswoche(&fixture(UNTERSTUETZUNG), BMI_VOLKSBEGEHREN_URL),
        None
    );
}

#[test]
fn detailseite_liefert_wortlaut_und_dokumente() {
    let mut volksbg = Volksbg {
        id: "gratis_verhuetung".to_string(),
        overview_url: "https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html"
            .to_string(),
        ..Default::default()
    };

    let overview_url = volksbg.overview_url.clone();
    merge_volksbg_detail(&mut volksbg, &fixture(DETAIL), &overview_url);

    assert_eq!(volksbg.title, "GRATIS Verhütung");
    assert_eq!(volksbg.ruling_date, Some(date(2026, 1, 16)));
    assert_eq!(volksbg.cut_off_date, Some(date(2026, 5, 11)));

    let description = volksbg.description.unwrap();
    assert!(
        description.starts_with("Prävention, Zugänglichkeit, Aufklärung"),
        "{description}"
    );
    assert!(description.contains("Kostenübernahme von Verhütungsberatung"));
    assert!(
        !description.contains("Gesetzgebungsperiode"),
        "{description}"
    );

    let links = volksbg
        .documents
        .iter()
        .map(|document| document.link.rsplit('/').next().unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        links,
        [
            "erledigung-sektion-iii-bwb_extern.pdf",
            "entscheidung_des_bundesministeriums_fuer_inneres_am_16_jaenner_2026_gratis_verhuetung.pdf",
            "vb101c_gratisverhuetung_begruendung_v4_bf.pdf",
            "broschuere-gratis-verhuetung_20260713.pdf",
            "csv-datei-gratis-verhuetung_20260713.csv"
        ]
    );

    assert!(volksbg
        .documents
        .iter()
        .all(|document| !document.title.is_empty()));
}

#[test]
fn dokumentenlinks_sind_aktiviert() {
    let mut volksbg = Volksbg {
        id: "gratis_verhuetung".to_string(),
        overview_url: detail_page("gratis_verhuetung"),
        ..Default::default()
    };

    let overview_url = volksbg.overview_url.clone();
    merge_volksbg_detail(&mut volksbg, &fixture(DETAIL), &overview_url);

    assert!(volksbg.documents.iter().all(|document| document
        .link
        .starts_with("https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/files/")));
}

#[test]
fn altes_cms_liefert_alle_dokumente_mit_demrichtigen_dateitypen() {
    let mut volksbg = Volksbg {
        id: "arbeitslosengeld_rauf".to_string(),
        ..Default::default()
    };

    merge_volksbg_detail(
        &mut volksbg,
        &fixture(DETAIL_ALTE_RS),
        &detail_page("Arbeitslosengeld_RAUF"),
    );

    assert_eq!(volksbg.title, "Arbeitslosengeld RAUF!");
    assert_eq!(volksbg.ruling_date, Some(date(2022, 2, 11)));
    assert_eq!(volksbg.cut_off_date, Some(date(2022, 3, 28)));

    let documents = volksbg
        .documents
        .iter()
        .map(|document| {
            (
                document.link.rsplit('/').next().unwrap().to_string(),
                document.ftype,
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        documents,
        [
            (
                "Verlautbarung_Arbeitslosengeld_Rauf.pdf".to_string(),
                FileType::OriginalPDF
            ),
            (
                "Stattgebung_Arbeitslosengeld_rauf_20220211.pdf".to_string(),
                FileType::OriginalPDF
            ),
            (
                "Begruendung_Arbeitslosengeld_rauf_20221102.pdf".to_string(),
                FileType::OriginalPDF
            ),
            (
                "Broschuere_Arbeitslosengeld_rauf_E.pdf".to_string(),
                FileType::OriginalPDF
            ),
            (
                "Statistik_Arbeitslosengeld_rauf_E.csv".to_string(),
                FileType::Unknown
            ),
        ]
    );

    assert!(volksbg.documents.iter().all(|document| document
        .link
        .starts_with("https://www.bmi.gv.at/411/Volksbegehren_der_XX_Gesetzgebungsperiode/Arbeitslosengeld_RAUF/files/")));
}

#[test]
fn jedes_detail_dokument_erscheint_genau_einmal() {
    for (fixture_name, page, documents) in [
        (DETAIL_ALTE_RS, "Arbeitslosengeld_RAUF", 5),
        (DETAIL_ANTI_GENDERN, "Anti-gendern-Volksbegehren", 5),
        (DETAIL_ASYL, "Asyl_europagerecht_umsetzen", 5),
    ] {
        let mut volksbg = Volksbg::default();
        merge_volksbg_detail(&mut volksbg, &fixture(fixture_name), &detail_page(page));

        assert_eq!(volksbg.documents.len(), documents, "{fixture_name}");

        let unique = volksbg
            .documents
            .iter()
            .map(|document| document.link.clone())
            .collect::<std::collections::HashSet<_>>();

        assert_eq!(unique.len(), documents, "{fixture_name}");
        assert!(volksbg
            .documents
            .iter()
            .all(|document| !document.title.is_empty()));
    }
}

#[test]
fn wortlaut_endet_vor_dem_gesamtergebnis() {
    let mut volksbg = Volksbg::default();
    merge_volksbg_detail(
        &mut volksbg,
        &fixture(DETAIL_ANTI_GENDERN),
        &detail_page("Anti-gendern-Volksbegehren"),
    );

    let description = volksbg.description.unwrap();

    assert!(
        description.starts_with(r#""Der Nationalrat wolle ein Bundesverfassungsgesetz"#),
        "{description}"
    );
    assert!(
        !description.contains("Begründung zum Volksbegehren"),
        "{description}"
    );
    assert!(
        !description.contains("Unterstützungserklärungen"),
        "{description}"
    );
    assert!(
        !description.contains("Volksbegehren seit der"),
        "{description}"
    );
    assert!(volksbg.documents.len() >= 3);
}

#[test]
fn wortlaut_und_dokumente_fehlen_duerfen_nicht_crashen() {
    assert_eq!(parse_wortlaut(&fixture(UNTERSTUETZUNG)), None);

    let mut volksbg = Volksbg {
        id: "ohne_dokumente".to_string(),
        ..Default::default()
    };

    merge_volksbg_detail(
        &mut volksbg,
        "<html><body><h1>Test</h1></body></html>",
        "https://www.bmi.gv.at/411/test/start.html",
    );

    assert_eq!(volksbg.title, "Test");
    assert_eq!(volksbg.description, None);
    assert!(volksbg.documents.is_empty());
    assert_eq!(volksbg.ruling_date, None);
}

#[test]
fn links_auf_alle_volksbg_des_bmi_verzeichnisses() {
    let links = parse_volksbg_links(&fixture(GP_INDEX), BMI_VOLKSBEGEHREN_INDEX_URL);

    assert_eq!(links.len(), 97);

    assert_eq!(links[0].id, "gratis_verhuetung");
    assert_eq!(links[0].title, "GRATIS Verhütung");
    assert_eq!(
        links[0].overview_url,
        "https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html"
    );

    assert!(links.iter().all(|volksbg| !volksbg.id.is_empty()));
    assert!(links.iter().all(|volksbg| !volksbg.title.is_empty()));
    assert!(links
        .iter()
        .all(|volksbg| !volksbg.overview_url.contains('#')));

    let unique = links
        .iter()
        .map(|volksbg| volksbg.id.clone())
        .collect::<std::collections::HashSet<_>>();

    assert_eq!(unique.len(), links.len());
}

#[test]
fn links_aus_dem_aktuellen_kasten() {
    let links = parse_volksbg_links(&fixture(LAUFEND), BMI_VOLKSBEGEHREN_URL);

    assert_eq!(
        links
            .iter()
            .map(|volksbg| volksbg.id.as_str())
            .collect::<Vec<_>>(),
        [
            "karfreitag_feiertag_fuer_alle",
            "polizei_-_kritischer_personalmangel",
            "wahlpflicht_nationalratswahl_bundespraesidentenwahl",
            "gratis_verhuetung",
            "transparenz_im_parlament"
        ]
    );

    assert_eq!(links[3].title, "GRATIS Verhütung");
}

#[test]
fn links_sind_kosten_und_zeitneutral() {
    let with_fragment = parse_volksbg_links(&fixture(GP_INDEX), BMI_VOLKSBEGEHREN_INDEX_URL);
    let again = parse_volksbg_links(&fixture(GP_INDEX), BMI_VOLKSBEGEHREN_INDEX_URL);

    assert_eq!(with_fragment, again);
    assert_eq!(
        with_fragment
            .iter()
            .filter(|volksbg| volksbg.id == "klimavolksbegehren")
            .count(),
        1
    );
}

#[test]
fn detailseite_nach_stattgebung_ohne_hashtag_links() {
    let mut volksbg = Volksbg {
        id: "GRATIS_Verhuetung".to_string(),
        ..Default::default()
    };

    let overview_url = volksbg.overview_url.clone();
    merge_volksbg_detail(
        &mut volksbg,
        &fixture(DETAIL_NACH_STATTGEBUNG),
        &overview_url,
    );

    assert_eq!(volksbg.ruling_date, Some(date(2026, 1, 16)));
    assert_eq!(volksbg.cut_off_date, Some(date(2026, 5, 11)));
    assert!(volksbg
        .description
        .unwrap()
        .contains("Gratis Verhütung für ALLE!"));
}

#[test]
fn deutsche_datumsbereiche() {
    assert_eq!(
        parse_german_date_range("Eintragungszeitraum vom 15. Juni bis 22. Juni 2026"),
        Some((date(2026, 6, 15), date(2026, 6, 22)))
    );
    assert_eq!(
        parse_german_date_range("vom 31. März bis 7. April 2025"),
        Some((date(2025, 3, 31), date(2025, 4, 7)))
    );
    assert_eq!(
        parse_german_date_range("vom 11. September bis 18. September 2024"),
        Some((date(2024, 9, 11), date(2024, 9, 18)))
    );
    assert_eq!(parse_german_date_range("15. Juni 2026"), None);
    assert_eq!(
        parse_german_date("22. Jänner 2025"),
        Some(date(2025, 1, 22))
    );
}

#[test]
fn ids_aus_unterschiedlichen_linkformaten() {
    assert_eq!(
        volksbg_id("volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html"),
        Some("gratis_verhuetung".to_string())
    );
    assert_eq!(
        volksbg_id("volksbegehren_der_xx_gesetzgebungsperiode/gratis_verhuetung/start.html#pk_01"),
        Some("gratis_verhuetung".to_string())
    );
    assert_eq!(
        volksbg_id(
            "https://web.archive.org/web/20260216/Volksbegehren_der_XX_Gesetzgebungsperiode/GRATIS_Verhuetung/start.aspx"
        ),
        Some("GRATIS_Verhuetung".to_string())
    );
    assert_eq!(
        absolute_url("files/text.pdf", "https://www.bmi.gv.at/411/start.html"),
        "https://www.bmi.gv.at/411/files/text.pdf"
    );
    assert_eq!(
        absolute_url(
            "411/Volksbegehren_der_XX_Gesetzgebungsperiode/x/files/y.pdf",
            "https://www.bmi.gv.at/411/volksbegehren_der_xx_gesetzgebungsperiode/x/start.html"
        ),
        "https://www.bmi.gv.at/411/Volksbegehren_der_XX_Gesetzgebungsperiode/x/files/y.pdf"
    );
    assert_eq!(
        absolute_url(
            "../files/text.pdf",
            "https://www.bmi.gv.at/411/x/start.html"
        ),
        "https://www.bmi.gv.at/411/x/files/text.pdf"
    );
}
