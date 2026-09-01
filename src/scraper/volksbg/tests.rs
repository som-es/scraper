use chrono::NaiveDate;
use common_scrapes::{Volksbg, VolksbgState};

use super::*;

const ANGEKUENDIGT: &str = "2026-05-08_wayback__bmi411__eintragungszeitraum_angekuendigt.html";
const LAUFEND: &str = "2026-06-14_wayback__bmi411__eintragungszeitraum_laufend.html";
const LAUFEND_SPAETER_TAG: &str = "2026-06-18_wayback__bmi411__eintragungszeitraum_laufend.html";
const ERGEBNISSE: &str = "2026-08-20_wayback__bmi411__ergebnisse_festgestellt.html";
const UNTERSTUETZUNG: &str = "2026-09-01__bmi411__unterstuetzung_und_ergebnisse.html";
const DETAIL: &str = "2026-09-01__bmi411__detail_gratis_verhuetung.html";
const DETAIL_NACH_STATTGEBUNG: &str =
    "2026-02-16_wayback__bmi411__detail_gratis_verhuetung_nach_stattgebung.html";

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

    merge_volksbg_detail(&mut volksbg, &fixture(DETAIL));

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
fn detailseite_nach_stattgebung_ohne_hashtag_links() {
    let mut volksbg = Volksbg {
        id: "GRATIS_Verhuetung".to_string(),
        ..Default::default()
    };

    merge_volksbg_detail(&mut volksbg, &fixture(DETAIL_NACH_STATTGEBUNG));

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
}
