use std::ops::Deref;

use ::scraper::{ElementRef, Html, Selector};

mod config;
mod document;
pub mod error;
mod request;
mod scraper;
mod speeches;
mod text_extractor;
mod url;

pub use text_extractor::*;

pub use document::*;

pub use common_scrapes::*;

pub use crate::scraper::*;
pub mod serde_json {
    pub use ::serde_json::*;
}

/// /person/83153 -> 83153
fn person_url_to_id(url: &str) -> Option<u64> {
    url.split('/').last().and_then(|id| id.parse::<u64>().ok())
}

pub type DeclineUrlGegenstand = (String, String, Option<u32>);
fn extract_from_gegenstand_url(text: &str) -> Option<DeclineUrlGegenstand> {
    let start = text.find('"')?;
    let (end, _) = text.char_indices().rev().find(|(_, x)| x == &'"')?;
    let mut gegenstand_url = text[start + 2..end].split('/');
    gegenstand_url.next();
    let gp = gegenstand_url.next()?.to_string();
    let ityp = gegenstand_url.next()?.to_string();
    let inr = gegenstand_url.next()?.to_string();
    Some((gp, ityp, inr.parse::<u32>().ok()))
}

pub fn extract_text_from_html(html: &str) -> String {
    let fragment = Html::parse_fragment(html);

    fragment
        .tree
        .nodes()
        .flat_map(|node| node.value().as_text().map(|text| text.deref()))
        .collect::<String>()
}

/// used if the id does not directly point to the text, but to an element that is completely outside the text
pub fn extract_text_from_html_till_next_name_attribute(html: &str, name_attribute: &str) -> String {
    let document = Html::parse_document(html);

    let selector = Selector::parse(&format!("[name=\"{name_attribute}\"]")).unwrap();

    let el = ElementRef::wrap(
        document
            .select(&selector)
            .next()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap(),
    )
    .unwrap();
    let start_at_tree_traversal = el.id();

    let nodes = el
        .tree()
        .nodes()
        .skip_while(|node| node.id() != start_at_tree_traversal);

    let mut count = 0;
    // for node in el.traverse() {

    let mut spoken_text = String::new();

    for node in nodes {
        match node.value() {
            ::scraper::Node::Element(el) => {
                let thing = el.attrs().find(|x| match x {
                    (name, _) if *name == "name" => true,
                    _ => false,
                });

                // println!("thing: {thing:?}");
                if let Some(thing) = thing {
                    if !thing.1.starts_with("RU") {
                        continue;
                    }
                    if count == 1 {
                        break;
                    }
                    count += 1;
                } else {
                    // get inner text
                    // let text = node.children().flat_map(|node| node.value().as_text().map(|text| text.deref())).collect::<String>();
                }
            }
            ::scraper::Node::Text(el) => {
                // let start = node.value().as_text().map(|text| text.deref());
                spoken_text.push_str(el.deref());
            }
            _ => continue,
        };

        /*match node {
            ego_tree::iter::Edge::Open(node) => {

            },
            _ => continue
        }*/
    }
    spoken_text
}

pub fn extract_text_from_html_via_name_attribute(html: &str, name_attribute: &str) -> String {
    let document = Html::parse_document(html);

    let selector = Selector::parse(&format!("[name=\"{}\"]", name_attribute)).unwrap();

    document
        .select(&selector)
        .flat_map(|node| {
            Some(
                ElementRef::wrap(node.parent()?.parent()?)?
                    .text()
                    .collect::<String>(),
            )
        })
        .collect::<String>()
}

#[cfg(test)]
mod tests {
    use crate::{
        extract_text_from_html_till_next_name_attribute, extract_text_from_html_via_name_attribute,
    };

    #[test]
    fn test_extract_text_from_html_via_name_attribute() {
        let url =
            "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/43/fnameorig_845422.html#RU_221617";
        let html = reqwest::blocking::get(url).unwrap();
        let html = html.text().unwrap();

        let text = extract_text_from_html_via_name_attribute(&html, "RU_221617");
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
}
