use async_trait::async_trait;
use common_scrapes::{LegislativeInitiative, Speech, Vote};
use regex::Regex;
use serde_json::{json, Value};

use crate::{
    documents,
    error::ScraperError,
    extract_text_from_html, person_url_to_id,
    speeches::{extract_speech, extract_speeches_from_reden, extract_speeches},
    AustriaScraper, GegenstandPaths, Scraper,
};

#[async_trait]
pub trait LegislativeInitiatives: Scraper {
    const FILTER_PATH: &'static str;

    fn legis_payload(&self) -> Self::Payload;

    async fn post_legis_payload(&self) -> Result<Value, ScraperError> {
        let payload = self.legis_payload();
        self.post_payload(
            payload,
            format!(
                "{base_url}{filter}",
                base_url = Self::BASE_URL,
                filter = Self::FILTER_PATH
            ),
        )
        .await
    }

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_legis_into_legis(raw_proposal).ok_or(ScraperError::LegisInitParseError)
    }
}

impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=10103&showAll";

    fn legis_payload(&self) -> Self::Payload {
        json!({"GP": self.scrape_info.gp, "NRBR": self.scrape_info.council_type})
    }

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        let legis_content = &raw_legis["content"];

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let inr = legis_content["inr"].as_u64()?;

        let date = legis_content
            .get("update")
            .unwrap_or(&legis_content["einlangen"])
            .as_str()?
            .to_string();

        let gp = legis_content["gp_code"].as_str()?.to_string();

        let raw_documents = if let Some(preconst) = legis_content.get("preconst") {
            let preconst = &preconst.as_array()?[0];
            preconst["documents"].as_array()?
        } else {
            legis_content["documents"].as_array()?
        };
        let documents = documents(raw_documents)?;

        let names = legis_content["names"]
            .as_array()
            .cloned()
            .unwrap_or_default();

        let delegate_ids = names
            .iter()
            .flat_map(|name| person_url_to_id(name["url"].as_str()?))
            .collect::<Vec<u64>>();

        let votes = extract_votes(legis_content)?;
        let speeches = extract_speeches(legis_content).unwrap_or_default();

        let accepted = legis_content["vote"]["infavor"].as_bool()?;

        let emphasis = if let Some(shortinfo) = legis_content.get("shortinfo") {
            let teil1 = extract_text_from_html(shortinfo["teil1"].as_str()?);
            let teil2 = extract_text_from_html(shortinfo["teil2"].as_str()?);
            Some(format!("{teil1} {teil2}"))
        } else {
            None
        };

        let id = format!("{ityp}/{inr}");

        Some(LegislativeInitiative {
            title,
            description,
            emphasis,
            // council: todo!(),
            gp,
            ityp,
            id,
            votes,
            documents,
            delegate_ids,
            accepted,
            date,
            speeches,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12)
            .ok_or(ScraperError::ProposalParseError)
    }
}


fn extract_votes(legis_content: &Value) -> Option<Vec<Vote>> {
    Some(
        legis_content["vote"]["result"]
            .as_array()?
            .iter()
            .flat_map(|vote| {
                Some(Vote {
                    party: vote["text"].as_str()?.to_string(),
                    fraction: vote["fraction"].as_u64()? as u32,
                    infavor: vote["infavor"].as_bool()?,
                })
            })
            .collect::<Vec<Vote>>(),
    )
}

// not working
fn remove_html_tags(html: &str) -> String {
    lazy_static::lazy_static! {
        static ref HTML_REGEX: Regex = Regex::new(r"<.*>").unwrap();
    }
    HTML_REGEX.replace_all(html, "").to_string()
}

#[cfg(test)]
mod tests {
    use crate::{
        extract_text_from_html,
        legis_init::{LegislativeInitiatives, Vote},
        AustriaScraper,
    };

    #[test]
    fn test_text_extract() {
        let html = r#"<h4>Schwerpunkte der Regierungsvorlage</h4>\n\n<ul class=\"unordered-list\">\n\t<li>&Auml;nderung des &sect; 46 KFG und Schaffung exakterer Vorgaben f&uuml;r die Bewilligung von &Uuml;berstellungsfahrten</li>\n\t<li>Entsprechende &Auml;nderung des &sect; 40 und des &sect; 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister &uuml;bernehmen k&ouml;nnen. Zus&auml;tzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgef&uuml;hrt werden</li>\n</ul>\n"#;

        let text = extract_text_from_html(html);
        assert_eq!("Schwerpunkte der Regierungsvorlage\\n\\n\\n\\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\\n\\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\\n\\n", text);
    }

    #[tokio::test]
    async fn test_legis_init() {
        let scraper = AustriaScraper::default();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
        }
    }

    #[tokio::test]
    async fn test_get_legis_init() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            //println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                continue;
            };

            println!("id: {}, title: {}", init.id, init.title);
        }

        println!("elapsed: {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn test_get_legis_init_i_1954_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        assert_eq!(
            init.title,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.) | Parlament Österreich"
        );
        assert_eq!(
            init.description,
            "Kraftfahrgesetz, Änderung (41. KFG-Novelle) (1954 d.B.)"
        );

        assert_eq!(init.gp, "XXVII");
        assert_eq!(init.emphasis, Some("Schwerpunkte der Regierungsvorlage\n\n\n\tÄnderung des § 46 KFG und Schaffung exakterer Vorgaben für die Bewilligung von Überstellungsfahrten\n\tEntsprechende Änderung des § 40 und des § 47 KFG, damit die Zulassungsstellen die relevanten Daten aus dem Unternehmensregister übernehmen können. Zusätzlich soll zur Datenbereinigung vorweg ein Datenabgleich durchgeführt werden\n\n \n\tNeuregelung der Ausbildung und Prüfung des in den Fahrschulen eingesetzten Lehrpersonals, wobei ein stärkerer Praxisbezug hergestellt wird und auch eine verpflichtende Aus- und Weiterbildung vorgeschrieben wird\n\tSchaffung einer entsprechenden Bestimmung in § 123 Abs. 2a KFG, wodurch besonders geschulten Organen der Asfinag diese Befugnis eingeräumt wird\n\tIn § 125 KFG wird der Kreis der potentiellen § 125 –Sachverständigen erweitert und auf Personen mit Meisterprüfung im Kraftfahrzeugtechnikerhandwerk, die eine Reifeprüfung oder Berufsreifeprüfung erfolgreich bestanden haben, ausgedehnt\n\tIn § 134 Abs. 3c wird die Höhe des Organmandates für Verstöße gegen das sog. Handyverbot von 50 Euro auf 100 Euro angehoben und in § 134 Abs. 3d wird die Höhe des Organmandates für Verstöße gegen die Gurt- oder die Sturzhelmpflicht von 35 Euro auf 50 Euro angehoben\n\n".to_string()));
        assert_eq!(
            init.votes,
            vec![
                Vote {
                    party: "ÖVP".to_string(),
                    fraction: 71,
                    infavor: true
                },
                Vote {
                    party: "SPÖ".to_string(),
                    fraction: 40,
                    infavor: false
                },
                Vote {
                    party: "FPÖ".to_string(),
                    fraction: 30,
                    infavor: false
                },
                Vote {
                    party: "GRÜNE".to_string(),
                    fraction: 26,
                    infavor: true
                },
                Vote {
                    party: "NEOS".to_string(),
                    fraction: 15,
                    infavor: true
                }
            ]
        );

        assert!(init.accepted);

        assert_eq!(init.id, "I/1954");
        assert_eq!(init.ityp, "I".to_string());

        assert_eq!(init.speeches[0].delegate_id, 52687);
        assert_eq!(init.speeches[0].infavor.unwrap(), false);

        assert_eq!(init.speeches[1].delegate_id, 5680);
        assert_eq!(init.speeches[1].infavor.unwrap(), true);

        assert_eq!(init.speeches[2].delegate_id, 51557);
        assert_eq!(init.speeches[2].infavor.unwrap(), false);

        assert_eq!(init.speeches[3].delegate_id, 83150);
        assert_eq!(init.speeches[3].infavor.unwrap(), true);

        assert_eq!(init.speeches[4].delegate_id, 5653);
        assert_eq!(init.speeches[4].infavor, None);
    }

    #[tokio::test]
    async fn test_get_legis_init_a_3021_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3021")
            .await
            .unwrap();

        assert_eq!(init.id, "A/3021");
        assert_eq!(init.ityp, "A");
        assert_eq!(
            init.title,
            "Arbeitsvertragsrechts-Anpassungsgesetz, Änderung (3021/A) | Parlament Österreich"
        );

        assert_eq!(
            init.description,
            "Arbeitsvertragsrechts-Anpassungsgesetz, Änderung (3021/A)"
        );
        assert_eq!(
            init.votes,
            [
                Vote {
                    party: "ÖVP".to_string(),
                    fraction: 71,
                    infavor: true
                },
                Vote {
                    party: "SPÖ".to_string(),
                    fraction: 40,
                    infavor: true
                },
                Vote {
                    party: "FPÖ".to_string(),
                    fraction: 30,
                    infavor: true
                },
                Vote {
                    party: "GRÜNE".to_string(),
                    fraction: 26,
                    infavor: true
                },
                Vote {
                    party: "NEOS".to_string(),
                    fraction: 15,
                    infavor: false
                }
            ]
        )
    }
}
