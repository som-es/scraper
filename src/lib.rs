use std::collections::HashSet;

use regex::Regex;

mod caching;
mod last_updated;
pub use last_updated::*;
mod config;
mod document;
pub mod error;
pub mod filter;
mod request;
mod scraper;
mod speeches;
mod text_extractor;
pub mod text_utils;

pub use text_extractor::*;

pub use document::*;

pub use common_scrapes::*;

pub use crate::scraper::*;
pub mod serde_json {
    pub use ::serde_json::*;
}

pub fn http_client() -> reqwest::Result<reqwest::Client> {
    reqwest::Client::builder()
        .user_agent("somes/2026.1.0")
        .build()
}

fn convert_vienna_datetime_to_utc(
    datetime: chrono::NaiveDateTime,
) -> Option<chrono::DateTime<chrono::Utc>> {
    use chrono::TimeZone;
    let vienna = chrono_tz::Europe::Vienna;
    vienna
        .from_local_datetime(&datetime)
        .earliest()
        .map(|dt| dt.to_utc())
}

/// /person/83153 -> 83153
fn person_url_to_id(url: &str) -> Option<u64> {
    url.split('/')
        .next_back()
        .and_then(|id| id.parse::<u64>().ok())
}

pub fn remove_page_info(haystack: &str) -> String {
    let re =
        Regex::new(r"Home[\S\s]*?Seite \d+[\S\s]*?Vorherige Seite[\S\s]*?Nächste Seite[\S\s]*?")
            .unwrap();
    re.replace_all(haystack, "").to_string()
}

pub fn remove_steno_info(haystack: &str, re: &Regex) -> String {
    // let re = Regex::new(r"Nationalrat, [\S\s]*Stenographisches Protokoll[\S\s]*\d+").unwrap();
    // Nationalrat,[\s\S]*?Stenographisches Protokoll[\s\S]*?/.+?\d+
    // let re = Regex::new(r"Nationalrat,[\s\S]*?Stenographisches Protokoll[\s\S]*?[Seite]{5}? \d+").unwrap();
    re.replace_all(haystack, "").to_string()
}

pub fn remove_plenar_info(haystack: &str) -> String {
    // Parlamentarische Materialien
    haystack.replace("Parlamentarische Materialien", "")
}

pub fn yes_no_votes_regex() -> Regex {
    Regex::new(r"(?i)abg.*ne\s*St.*me.[:\s]*\s*(\d+)[^\d]*.*Ja.*-Sti.me.[:\s]*\s*(\d+)[^\d]*.*Nein.*-St.m.e.[:\s]*\s*(\d+)\s*(?:ung..+tig\s*(\d+))?")
        .unwrap()
}

pub fn yes_no_votes_inside_regex() -> Regex {
    Regex::new(r"(?i).*Ja.*-Sti.men[:\s]*\s*(\d+)[^\d]*.*Nein.*-St.m.en[:\s]*\s*(\d+)\s*(?:ung..+tig\s*(\d+))?")
        .unwrap()
}

pub fn remove_titles_from_str(text: &str) -> String {
    let titles = unique_titles();

    text.split_inclusive(' ')
        .map(|x| x.trim_matches(char::is_whitespace).trim_matches(','))
        .filter(|x| !titles.contains(x.to_lowercase().as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

#[rustfmt::skip]
fn unique_titles() -> HashSet<&'static str> {
    [
        // Original titles
        "Dr.", "Mag.", "Dipl.-Kfm.", "Dipl.-Kfm. Dr.", "Dipl.-Kfm. Mag.", "Dipl.-Ing.",
        "Dipl.-Ing. Dr.", "Dipl.-Ing. (FH)", "Dipl.-Kffr. (FH)", "Dipl.-Vw.", "Dipl.-Vw. Dr.",
        "Dipl.-Ing., MBA", "Dipl.-Ing., MAS", "Dipl.-Kfm., MBA", "DDr.", "Dr. Dr. h.c.",
        "Dr. Dr. h.c. mult.", "Dr. h.c.", "Dr., MBA", "Dr., LL.M.", "Dr., MBL-HSG",
        "Mag. Dr.", "Mag. (FH)", "Mag., BA", "Mag., MA", "Mag., MBA", "Mag. Dr., BA",
        "Mag. Dr., MSc", "Mag. Dr., MBL-HSG", "MMag.", "MMag. Dr.", "MMag., BA", "MMag., LL.M.",
        "MA", "MA MLS", "MSc", "MSc (WU)", "MBA", "BSc", "BSc (WU)", "BA", "BA MA", "BA MA MA",
        "BEd", "BEd BEd", "MA, MAS", "MPA", "MES", "MMSc BA", "MEd MA", "MMMag. Dr.",
        "MMMag.", "abs. iur.", "Freiherr von", "Ritter", "Graf", "Edler von",
        "LL.M.", "LL.M. (WU)", "LL.M. (LSE)", "LL.M. (King’s College London)",
        "MBL-HSG", "MAS", "MMag.", "MMag. Dr.", "MMst. Mag. (FH)", "lic.oec. HSG",
        "Dr. h.c. mult.", "Dr. Dr.", "Dipl.-Kfm.", "Dipl.-Kfm. Dr.", "Dipl.-Ing. Dr.",
        "Dipl.-Kfm. Dr. h.c.", "Dipl.-Vw. Dr.", "Dipl.-Ing. Mag.", "MSc BA", "MA MLS", "MA, MSc",
        "Dr. Dr. h.c. mult.", "Dipl.-Kfm.", "Ing.", "Ing. Mag.", "Ing. Dr.", "Ing. MSc",
        "Mst.", "Mst. DI (FH)", "Mag. (FH)", "PMM", "abs. iur.", "Edler von", "Ritter von",
        "BM", "StS",

        "BM", "StS", "MEP", "PRRH", "VA", "VK", "BK", "GAST",
        "Dr.", "Mag.", "Dipl.-Kfm.", "Dipl.-Ing.", "Dipl.-Kffr.", "Dipl.-Vw.",
        "DDr.", "Dr. Dr. h.c.", "Dr. Dr. h.c. mult.", "Dr. h.c.", "Dr. h.c. mult.", "Dr. Dr.",
        "Mag. Dr.", "Mag. (FH)", "MMag.", "MMag. Dr.", "MMMag.", "MMMag. Dr.",
        "MA", "MSc", "MBA", "BSc", "BA", "BEd", "MPA", "MES", "MBL-HSG", "MAS", "PMM",
        "LL.M.", "abs. iur.", "lic.oec. HSG",
        "Freiherr von", "Ritter", "Ritter von", "Graf", "Edler von",
        "Ing.", "Mst.", "MMst.",
        "Dr., MBA", "Dr., LL.M.", "Dr., MBL-HSG", "Mag., BA", "Mag., MA", "Mag., MBA",
        "Dipl.-Ing., MBA", "Dipl.-Ing., MAS", "Dipl.-Kfm., MBA",
        "Mag. Dr., BA", "Mag. Dr., MSc", "Mag. Dr., MBL-HSG",
        "MMag., BA", "MMag., LL.M.", "MMst. Mag. (FH)",
        "MA MLS", "MSc (WU)", "BSc (WU)", "BA MA", "BA MA MA", "BEd BEd",
        "MA, MAS", "MMSc BA", "MEd MA", "MSc BA", "MA, MSc",
        "LL.M. (WU)", "LL.M. (LSE)", "LL.M. (King’s College London)",
        "Mst. DI (FH)", "Ing. Mag.", "Ing. Dr.", "Ing. MSc",
        "Dipl.-Ing. (FH)", "Dipl.-Kffr. (FH)", "Dipl.-Ing. Dr.", "Dipl.-Ing. Mag.",
        "Dipl.-Kfm. Dr.", "Dipl.-Kfm. Mag.", "Dipl.-Vw. Dr.",
        "Dipl.-Kfm. Dr. h.c.",
        "Mst.", "Ing. Mst. DI (FH)",

        // Lowercased titles
        "dr.", "mag.", "dipl.-kfm.", "dipl.-kfm. dr.", "dipl.-kfm. mag.", "dipl.-ing.",
        "dipl.-ing. dr.", "dipl.-ing. (fh)", "dipl.-kffr. (fh)", "dipl.-vw.", "dipl.-vw. dr.",
        "dipl.-ing., mba", "dipl.-ing., mas", "dipl.-kfm., mba", "ddr.", "dr. dr. h.c.",
        "dr. dr. h.c. mult.", "dr. h.c.", "dr., mba", "dr., ll.m.", "dr., mbl-hsg",
        "mag. dr.", "mag. (fh)", "mag., ba", "mag., ma", "mag., mba", "mag. dr., ba",
        "mag. dr., msc", "mag. dr., mbl-hsg", "mmag.", "mmag. dr.", "mmag., ba", "mmag., ll.m.",
        "ma", "ma mls", "msc", "msc (wu)", "mba", "bsc", "bsc (wu)", "ba", "ba ma", "ba ma ma",
        "bed", "bed bed", "ma, mas", "mpa", "mes", "mmsc ba", "med ma", "mmmag. dr.",
        "mmmag.", "abs. iur.", "freiherr von", "ritter", "graf", "edler von",
        "ll.m.", "ll.m. (wu)", "ll.m. (lse)", "ll.m. (king’s college london)",
        "mbl-hsg", "mas", "mmag.", "mmag. dr.", "mmst. mag. (fh)", "lic.oec. hsg",
        "dr. h.c. mult.", "dr. dr.", "dipl.-kfm.", "dipl.-kfm. dr.", "dipl.-ing. dr.",
        "dipl.-kfm. dr. h.c.", "dipl.-vw. dr.", "dipl.-ing. mag.", "msc ba", "ma mls", "ma, msc",
        "dr. dr. h.c. mult.", "dipl.-kfm.", "ing.", "ing. mag.", "ing. dr.", "ing. msc",
        "mst.", "mst. di (fh)", "mag. (fh)", "pmm", "abs. iur.", "edler von", "ritter von", "bm", "sts",

        "bm", "sts", "mep", "prrh", "va", "vk", "bk", "gast",
        "dr.", "mag.", "dipl.-kfm.", "dipl.-ing.", "dipl.-kffr.", "dipl.-vw.",
        "ddr.", "dr. dr. h.c.", "dr. dr. h.c. mult.", "dr. h.c.", "dr. h.c. mult.", "dr. dr.",
        "mag. dr.", "mag. (fh)", "mmag.", "mmag. dr.", "mmmag.", "mmmag. dr.",
        "ma", "msc", "mba", "bsc", "ba", "bed", "mpa", "mes", "mbl-hsg", "mas", "pmm",
        "ll.m.", "abs. iur.", "lic.oec. hsg",
        "freiherr von", "ritter", "ritter von", "graf", "edler von",
        "ing.", "mst.", "mmst.",
        "dr., mba", "dr., ll.m.", "dr., mbl-hsg", "mag., ba", "mag., ma", "mag., mba",
        "dipl.-ing., mba", "dipl.-ing., mas", "dipl.-kfm., mba",
        "mag. dr., ba", "mag. dr., msc", "mag. dr., mbl-hsg",
        "mmag., ba", "mmag., ll.m.", "mmst. mag. (fh)",
        "ma mls", "msc (wu)", "bsc (wu)", "ba ma", "ba ma ma", "bed bed",
        "ma, mas", "mmsc ba", "med ma", "msc ba", "ma, msc",
        "ll.m. (wu)", "ll.m. (lse)", "ll.m. (king’s college london)",
        "mst. di (fh)", "ing. mag.", "ing. dr.", "ing. msc",
        "dipl.-ing. (fh)", "dipl.-kffr. (fh)", "dipl.-ing. dr.", "dipl.-ing. mag.",
        "dipl.-kfm. dr.", "dipl.-kfm. mag.", "dipl.-vw. dr.",
        "dipl.-kfm. dr. h.c.",
        "mst.", "ing. mst. di (fh)"
    ]
    .into_iter()
    .collect()
}

pub type DeclineUrlGegenstand = (String, String, Option<u32>);

pub fn extract_from_gegenstand_url(url: &str) -> Option<DeclineUrlGegenstand> {
    let mut gegenstand_url = url.split('/');
    gegenstand_url.next();
    let gp = gegenstand_url.next()?.to_string();
    let ityp = gegenstand_url.next()?.to_string();
    let inr = gegenstand_url.next()?.to_string();
    Some((gp, ityp, inr.parse::<u32>().ok()))
}

pub fn extract_from_gegenstand_url_text(text: &str) -> Option<DeclineUrlGegenstand> {
    let start = text.find('"')?;
    let (end, _) = text.char_indices().rev().find(|(_, x)| x == &'"')?;
    let gegenstand_url = &text[start + 2..end];
    extract_from_gegenstand_url(gegenstand_url)
}

pub fn convert_party_code_list_to_vec(party_list: &str) -> Vec<String> {
    party_list
        .split(',')
        .map(|party| party.trim_matches(' '))
        .map(ToString::to_string)
        .collect()
}

#[inline]
pub fn today() -> chrono::NaiveDate {
    chrono::Local::now().date_naive()
}

pub fn convert_party_code_list_to_vec_open_ended(party_list_open_ended: &str) -> Vec<String> {
    let mut contra = vec![];

    'outer: for maybe_party_split in party_list_open_ended.split(',') {
        let mut party_code = String::new();
        for char in maybe_party_split.chars() {
            if char == ' ' || char == ',' {
                continue;
            }
            if char.is_uppercase() {
                party_code.push(char);
            } else {
                contra.push(party_code);
                break 'outer;
            }
        }
        contra.push(party_code);
    }
    contra
}

pub fn extract_pro_contra_party_codes_from_string(pro_contra_str: &str) -> Option<ProContra> {
    let pro_start = pro_contra_str.to_lowercase().find("dafür")?;
    let contra_start = pro_contra_str.to_lowercase().find("dagegen")?;

    let pro = &pro_contra_str[pro_start + 7..contra_start - 2];
    let pro = convert_party_code_list_to_vec(pro);
    let pro = pro.into_iter().filter(|x| !x.is_empty()).collect();

    let contra = convert_party_code_list_to_vec_open_ended(&pro_contra_str[contra_start + 8..]);
    let contra = contra.into_iter().filter(|x| !x.is_empty()).collect();

    Some(ProContra { pro, contra })
}

#[cfg(test)]
mod tests {
    use scraper::Html;

    use crate::{
        extract_pro_contra_party_codes_from_string,
        extract_text_from_html_till_next_name_attribute,
        extract_text_from_html_via_name_attribute_parent_parent, remove_titles_from_str,
    };

    #[test]
    fn test_extract_text_from_html_via_name_attribute() {
        let url =
            "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/43/fnameorig_845422.html#RU_221617";
        let html = reqwest::blocking::get(url).unwrap();
        let html = html.text().unwrap();

        let text = extract_text_from_html_via_name_attribute_parent_parent(
            &Html::parse_document(&html),
            "RU_221617",
        );
        assert_eq!(text, "\n15.38.53\nAbgeordnete\nMag.\u{a0}Andrea\u{a0}Kuntzl (SPÖ): Sehr\ngeehrte Damen und Herren! Wir hoffen sehr, dass das Blümel-Virus nicht am\nPräsidium angekommen ist, Herr Präsident!\nWir behandeln jetzt mit diesen Vorlagen Neuregelungen, die\neinerseits die Fachhoch\u{ad}schulen, andererseits die Privatuniversitäten\nund die pädagogischen Hochschulen be\u{ad}treffen.\nAlle Regelungen, die sicherstellen, die dafür sorgen,\ndass die Qualitätssicherung verbes\u{ad}sert wird, begrüßen wir.\nAlle Regelungen, die für mehr Transparenz sorgen, begrüßen \nNationalrat, XXVII.GPStenographisches Protokoll43. Sitzung, 7. Juli 2020 / Seite 143\nwir.\nWenn wir uns die kritischen Stellungnahmen anschauen, die vielen Briefe, die\nwir bekommen haben, Anrufe, die wir bekommen haben, Gespräche, die wir\ngeführt haben, stellen wir allerdings mit großer Sorge fest, dass\nBedenken, berechtigte Bedenken, die seitens der Leute, die in diesen Bereichen\narbeiten\u{a0}– qualifizierter Personen also\u{a0}–, be\u{ad}stehen,\neinfach negiert werden, dass da drübergefahren wird und dass sehr qualifizierte,\nengagierte Leute düpiert werden. Da hätte man Gespräche\nführen und sicher noch das eine oder andere ändern können.\nIch werde mich jetzt mit den Bereichen der\nFachhochschulen und der Privatuniversitäten beschäftigen. Im Bereich\nder Fachhochschulen ist ausdrücklich zu begrüßen, dass jetzt im\nGesetz der Fachhochschulentwicklungs- und Finanzierungsplan explizit genannt\nwird. Das ist wichtig, das war für die Fachhochschulen immer eine\nZitterpartie. Es ist gut, dass das jetzt festgelegt ist. Was uns fehlt, ist,\ndass das auch mit einem Begut\u{ad}achtungsverfahren verbunden ist und dass das\nParlament einbezogen wird. Das ist aus unserer Sicht ein schwerwiegender\nSchönheitsfehler, vielleicht ergibt sich das ja doch noch in den\nnächsten Jahren.\nAus unserer Sicht auch wichtig wäre eine\nBerichtslegung aus dem Fachhochschulsektor, so wie bei den Universitäten.\nVon diesen wird alle drei Jahre dem Parlament berichtet. Das würden wir\nauch für den Fachhochschulsektor sehr wichtig finden, um uns mit die\u{ad}sem\nwichtigen Bereich unseres Bildungssystems hier im Hohen Haus auch entspre\u{ad}chend\nauseinanderzusetzen.\nEin Punkt, den wir ausdrücklich ablehnen,\nist die Möglichkeit, die in diesem Gesetz geschaffen wird,\nFachhochschulstudienplätze, Studiengänge zu kaufen. Sie sind für\nde\u{ad}finierte, abgegrenzte Zielgruppen seitens außerhochschulischer,\nprivater Rechtsträger zu kaufen. Das läuft darauf hinaus, dass die\nFachhochschulen maßgeschneiderte, eng definierte Bildungsangebote\nanbieten. Das könnte auch zu einer finanziellen Abhängig\u{ad}keit von\ndiesen Auftraggebern führen. Außerdem ist aus unserer Sicht\nbesonders wich\u{ad}tig, dass, wenn man einen Fachhochschullehrgang, eine ‑ausbildung\nmacht, das vielsei\u{ad}tig und nicht nur für einen bestimmten Betrieb,\nfür ein bestimmtes Unternehmen einsetz\u{ad}bar ist.\nDas geschieht noch dazu in Verknüpfung\ndamit, dass die Veröffentlichung von Finan\u{ad}zierungsquellen auf\nöffentliche Zahlungen eingeschränkt ist und nicht auch private Zah\u{ad}lungen\numfasst. Das ist ein Gemisch, das wirklich kein gutes Bild bietet.\nBei den Privatuniversitäten ist ein Punkt,\nden wir sehr begrüßen, der Transparenz schafft, dass jetzt die\nAusbildungsverträge auf der Homepage der Privatuniversität ver\u{ad}öffentlicht\nwerden müssen. Was allerdings fehlt, ist, dass auch die Gesamtkosten des\nStudiums veröffentlicht werden, was ja gerade bei Privatuniversitäten\nund den hohen Kosten, die dort für die Studierenden entstehen, ein\nwichtiger Faktor wäre.\nEin Punkt, den wir in diesem Bereich sehr\nkritisch sehen, ist die Einführung der Privat\u{ad}hochschulen. Es soll\nkünftig also nicht nur Privatuniversitäten, sondern auch Privathoch\u{ad}schulen\ngeben, das heißt, es soll mit diesem Gesetz das Angebot an\nkostenpflichtigen Ausbildungen erhöht werden. Wir sehen aber in den\nletzten Jahren eine Entwicklung dahin gehend, dass kostenpflichtige Angebote\nsowieso ausgebaut und die Kosten er\u{ad}höht werden. Es ist aus diesem\nGesetzentwurf auch nicht ersichtlich, welches Profil die\u{ad}se\nPrivathochschulen eigentlich haben sollen und wer das eigentlich braucht. Wir\nsehen nicht, dass dafür ein wirklicher Bedarf besteht. Das erhöht\neinfach die Kleinteiligkeit, die Unübersichtlichkeit und vor allem die\nKommerzialisierung der Ausbildungsangebote im tertiären Bereich.\nWas wir dringend brauchen, ist ein Ausbau an\nFachhochschulstudienplätzen. Das wäre ein Gebot der Stunde, Herr\nBildungsminister, wenn ich Sie ansprechen darf, nicht nur für den\nWissenschaftsbereich. Wir wissen, dass die momentane Krise junge Menschen\n\nNationalrat, XXVII.GPStenographisches Protokoll43. Sitzung, 7. Juli 2020 / Seite 144\nbesonders trifft, dass es besonders schwierig ist, Lehrstellen zu finden,\nArbeitsplätze zu finden. Das Gebot der Stunde wäre, Herr\nBildungsminister, dass Sie ein Maßnahmen\u{ad}paket schnüren und im\ngesamten Bildungsbereich\u{a0}– den weiterführenden Schulen, den\nFachhochschulen, den Universitäten\u{a0}– zusätzliche\nAusbildungsangebote für junge Men\u{ad}schen zur Verfügung stellen.\nDas wäre sinnvoller, als wenn sie im Herbst auf der Straße stehen. (Beifall\nbei der SPÖ.)\n15.44\n");

        let text = extract_text_from_html_till_next_name_attribute(&html, "RU_221617");
        assert_eq!(text, "\n15.38.53\nAbgeordnete\nMag.\u{a0}Andrea\u{a0}Kuntzl (SPÖ): Sehr\ngeehrte Damen und Herren! Wir hoffen sehr, dass das Blümel-Virus nicht am\nPräsidium angekommen ist, Herr Präsident!\nWir behandeln jetzt mit diesen Vorlagen Neuregelungen, die\neinerseits die Fachhoch\u{ad}schulen, andererseits die Privatuniversitäten\nund die pädagogischen Hochschulen be\u{ad}treffen.\nAlle Regelungen, die sicherstellen, die dafür sorgen,\ndass die Qualitätssicherung verbes\u{ad}sert wird, begrüßen wir.\nAlle Regelungen, die für mehr Transparenz sorgen, begrüßen \nNationalrat, XXVII.GPStenographisches Protokoll43. Sitzung, 7. Juli 2020 / Seite 143\nwir.\nWenn wir uns die kritischen Stellungnahmen anschauen, die vielen Briefe, die\nwir bekommen haben, Anrufe, die wir bekommen haben, Gespräche, die wir\ngeführt haben, stellen wir allerdings mit großer Sorge fest, dass\nBedenken, berechtigte Bedenken, die seitens der Leute, die in diesen Bereichen\narbeiten\u{a0}– qualifizierter Personen also\u{a0}–, be\u{ad}stehen,\neinfach negiert werden, dass da drübergefahren wird und dass sehr qualifizierte,\nengagierte Leute düpiert werden. Da hätte man Gespräche\nführen und sicher noch das eine oder andere ändern können.\nIch werde mich jetzt mit den Bereichen der\nFachhochschulen und der Privatuniversitäten beschäftigen. Im Bereich\nder Fachhochschulen ist ausdrücklich zu begrüßen, dass jetzt im\nGesetz der Fachhochschulentwicklungs- und Finanzierungsplan explizit genannt\nwird. Das ist wichtig, das war für die Fachhochschulen immer eine\nZitterpartie. Es ist gut, dass das jetzt festgelegt ist. Was uns fehlt, ist,\ndass das auch mit einem Begut\u{ad}achtungsverfahren verbunden ist und dass das\nParlament einbezogen wird. Das ist aus unserer Sicht ein schwerwiegender\nSchönheitsfehler, vielleicht ergibt sich das ja doch noch in den\nnächsten Jahren.\nAus unserer Sicht auch wichtig wäre eine\nBerichtslegung aus dem Fachhochschulsektor, so wie bei den Universitäten.\nVon diesen wird alle drei Jahre dem Parlament berichtet. Das würden wir\nauch für den Fachhochschulsektor sehr wichtig finden, um uns mit die\u{ad}sem\nwichtigen Bereich unseres Bildungssystems hier im Hohen Haus auch entspre\u{ad}chend\nauseinanderzusetzen.\nEin Punkt, den wir ausdrücklich ablehnen,\nist die Möglichkeit, die in diesem Gesetz geschaffen wird,\nFachhochschulstudienplätze, Studiengänge zu kaufen. Sie sind für\nde\u{ad}finierte, abgegrenzte Zielgruppen seitens außerhochschulischer,\nprivater Rechtsträger zu kaufen. Das läuft darauf hinaus, dass die\nFachhochschulen maßgeschneiderte, eng definierte Bildungsangebote\nanbieten. Das könnte auch zu einer finanziellen Abhängig\u{ad}keit von\ndiesen Auftraggebern führen. Außerdem ist aus unserer Sicht\nbesonders wich\u{ad}tig, dass, wenn man einen Fachhochschullehrgang, eine ‑ausbildung\nmacht, das vielsei\u{ad}tig und nicht nur für einen bestimmten Betrieb,\nfür ein bestimmtes Unternehmen einsetz\u{ad}bar ist.\nDas geschieht noch dazu in Verknüpfung\ndamit, dass die Veröffentlichung von Finan\u{ad}zierungsquellen auf\nöffentliche Zahlungen eingeschränkt ist und nicht auch private Zah\u{ad}lungen\numfasst. Das ist ein Gemisch, das wirklich kein gutes Bild bietet.\nBei den Privatuniversitäten ist ein Punkt,\nden wir sehr begrüßen, der Transparenz schafft, dass jetzt die\nAusbildungsverträge auf der Homepage der Privatuniversität ver\u{ad}öffentlicht\nwerden müssen. Was allerdings fehlt, ist, dass auch die Gesamtkosten des\nStudiums veröffentlicht werden, was ja gerade bei Privatuniversitäten\nund den hohen Kosten, die dort für die Studierenden entstehen, ein\nwichtiger Faktor wäre.\nEin Punkt, den wir in diesem Bereich sehr\nkritisch sehen, ist die Einführung der Privat\u{ad}hochschulen. Es soll\nkünftig also nicht nur Privatuniversitäten, sondern auch Privathoch\u{ad}schulen\ngeben, das heißt, es soll mit diesem Gesetz das Angebot an\nkostenpflichtigen Ausbildungen erhöht werden. Wir sehen aber in den\nletzten Jahren eine Entwicklung dahin gehend, dass kostenpflichtige Angebote\nsowieso ausgebaut und die Kosten er\u{ad}höht werden. Es ist aus diesem\nGesetzentwurf auch nicht ersichtlich, welches Profil die\u{ad}se\nPrivathochschulen eigentlich haben sollen und wer das eigentlich braucht. Wir\nsehen nicht, dass dafür ein wirklicher Bedarf besteht. Das erhöht\neinfach die Kleinteiligkeit, die Unübersichtlichkeit und vor allem die\nKommerzialisierung der Ausbildungsangebote im tertiären Bereich.\nWas wir dringend brauchen, ist ein Ausbau an\nFachhochschulstudienplätzen. Das wäre ein Gebot der Stunde, Herr\nBildungsminister, wenn ich Sie ansprechen darf, nicht nur für den\nWissenschaftsbereich. Wir wissen, dass die momentane Krise junge Menschen\n\nNationalrat, XXVII.GPStenographisches Protokoll43. Sitzung, 7. Juli 2020 / Seite 144\nbesonders trifft, dass es besonders schwierig ist, Lehrstellen zu finden,\nArbeitsplätze zu finden. Das Gebot der Stunde wäre, Herr\nBildungsminister, dass Sie ein Maßnahmen\u{ad}paket schnüren und im\ngesamten Bildungsbereich\u{a0}– den weiterführenden Schulen, den\nFachhochschulen, den Universitäten\u{a0}– zusätzliche\nAusbildungsangebote für junge Men\u{ad}schen zur Verfügung stellen.\nDas wäre sinnvoller, als wenn sie im Herbst auf der Straße stehen. (Beifall\nbei der SPÖ.)\n15.44\n\n\n\n\nPräsident\nMag.\u{a0}Wolfgang\u{a0}Sobotka: Zu Wort gemeldet\nist Herr Abgeordneter Taschner.\u{a0}– Bitte. \n\n\n\n\n");
    }

    #[test]
    fn test_extract_text_from_html_via_name_attribute_missing_text() {
        let url =
            "https://www.parlament.gv.at/dokument/XXV/NRSITZ/186/fnameorig_673189.html#RU_173145";
        let html = reqwest::blocking::get(url).unwrap();
        let html = html.text().unwrap();

        let text = extract_text_from_html_till_next_name_attribute(&html, "RU_173145");
        assert_eq!(text, "\nPräsident Ing.\u{a0}Norbert\u{a0}Hofer: Zu\nWort gelangt nun Frau Abgeordnete Himmelbauer.\u{a0}– Bitte, Frau\nAbgeordnete.\n\u{a0}\n\n\n\n\n14.48.00\nAbgeordnete Eva-Maria\u{a0}Himmelbauer,\u{a0}BSc (ÖVP): Herr\nPräsident! Mitglieder der Bun\u{ad}desregierung! Werte Kolleginnen\nund Kollegen! Es freut mich auch, dass es heute zu einer Einigung im\nBildungspaket gekommen ist, dass hier ein Kompromiss gefunden wurde, der auch\ndie Eltern und Lehrer in eine Entscheidung mit einbindet. Ich glaube, es ist\nganz besonders wichtig, noch einmal hervorzuheben, dass da auch Mitsprache\nsichergestellt wird und dass die Politik den Wunsch der Eltern respektiert,\nwenn es darum geht, in welcher Form und unter welchem Dach ihre Kinder\nausgebildet werden.\nVielleicht ist es kein großer Schritt, aber ich glaube\ndennoch, es ist ein Schritt in die richtige Richtung, wie auch viele andere\nSachen ein Schritt in die richtige Richtung wa\u{ad}ren, sei es die\nBildungsinnovationsstiftung, sei es die Open Innovation Strategie, sei es die\nVerbesserung im Übergang zwischen Kindergarten und Volksschule, sei es bei\nder Sprachförderung. Und da der Herr Bundeskanzler auch den Themenbereich\nDigitalisie\u{ad}rung heute angesprochen hat: Auch da hat er unsere\nUnterstützung, nicht nur von der ÖVP, sondern auch vom Hohen Haus,\ndenn dazu haben wir einen Entschließungsan\u{ad}trag eingebracht, der die\nInfrastruktur, die Geräteausstattung, WLAN bis hin zur digita\u{ad}len\nKompetenz im Unterricht beinhaltet.\nIch kann hier aber auch nicht die Finanzierung der Unis\nauslassen, denn wenn wir über Bildung sprechen, dann reden wir über\ndie ganze Kette im Bildungswesen, vom Kin\u{ad}dergarten über die Schule,\ndie Lehre bis hin zum Studium. Auch unser Regierungspro\u{ad}gramm, das ja eigentlich noch bis 15.\u{a0}Oktober\nGültigkeit hat, sieht vor, dass wir die Stu\u{ad}dienplatzfinanzierung umsetzen. Und selbst\nBundeskanzler Kern\u{a0}– der damals zwar nicht bei den\nVerhandlungen dabei war\u{a0}– hat im Jänner diese Zustimmung noch\neinmal be\u{ad}kräftigt und sich\ndarüber hinaus auch für Zugangsregelungen ausgesprochen, da es um\ndie Qualität, die Betreuungssituation an den österreichischen\nUniversitäten geht.\nIch will hier gar nicht den Plan\u{a0}A\nzitieren, aber ein wichtiger Satz, der da drinnen steht und den heute auch der\nBundeskanzler angesprochen hat, war: Es geht nicht um so\u{ad}ziale Selektion, sondern um Leistungsselektion.\u{a0}–\nDem können wir uns anschließen. (Bei\u{ad}fall bei\nder ÖVP.)\nJenen Kräften\nin der SPÖ, denen das nicht so taugt und die das am liebsten bis zum\nWahltag aussitzen würden, kann ich nur sagen, das wäre eine fatale\nEntscheidung, denn es würde nicht nur bis zum 15.\u{a0}Oktober nichts\npassieren, sondern es würde die nächsten\ndrei Jahre nichts passieren, da bis dahin auch die Finanzierungsvereinbarung\nnicht stattfinden wird. Ich kann also nur an die konstruktiven Kräfte in\nder SPÖ appellieren: Setzen wir die Studienplatzfinanzierung und die\nZugangsregelungen um! (Beifall bei der ÖVP.)\n14.51\n\n\n\n\nPräsident\nIng.\u{a0}Norbert\u{a0}Hofer: Nächster Redner\nist Herr Dr.\u{a0}Rosenkranz.\u{a0}– Bitte, Herr Abgeordneter.\n");
    }

    #[test]
    fn test_pro_contra_extraction_end_string() {
        let text = "169. Sitzung des Nationalrates: Abstimmung: Antrag, den ablehnenden Ausschussbericht zur Kenntnis zu nehmen: <b>angenommen</b><br>Dafür: V, G. Dagegen: S, F, N";
        let party_codes = extract_pro_contra_party_codes_from_string(text).unwrap();
        assert_eq!(party_codes.pro, vec!["V".to_string(), "G".to_string()]);
        assert_eq!(
            party_codes.contra,
            vec!["S".to_string(), "F".to_string(), "N".to_string()]
        );
    }

    #[test]
    fn test_pro_contra_extraction_2_3_string() {
        let text = "
270. Sitzung des Nationalrates: Gesetzesvorschlag in zweiter Lesung abgelehnt
Dafür: V, G. Dagegen: S, F, N (Gesetzentwurf findet nicht die erforderliche 2/3 Mehrheit, somit kein Gesetzesbeschluss des NR im Sinne des § 82 Abs. 2 Z. 1 GOG)";

        let party_codes = extract_pro_contra_party_codes_from_string(text).unwrap();
        assert_eq!(party_codes.pro, vec!["V".to_string(), "G".to_string()]);
        assert_eq!(
            party_codes.contra,
            vec!["S".to_string(), "F".to_string(), "N".to_string()]
        );
    }

    #[test]
    fn test_remove_titles_back() {
        let name = "Dieter Brosz, Msc";

        let name = remove_titles_from_str(name);
        assert_eq!("Dieter Brosz", name);
    }

    #[test]
    fn test_remove_titles_front() {
        let name = "Dr. Dieter Brosz, Msc";

        let name = remove_titles_from_str(name);
        assert_eq!("Dieter Brosz", name);
    }
}
