use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::{LegislativeInitiative, Party, Vote};
use serde_json::{json, Value};

use crate::{
    documents, error::ScraperError, extract_text_from_html, person_url_to_id,
    speeches::extract_speeches, AustriaScraper, GegenstandPaths, Scraper,
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
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=10103&showAll=true";

    fn legis_payload(&self) -> Self::Payload {
        json!({"GP": self.scrape_info.gp, "NRBR": self.scrape_info.council_type})
    }

    fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        // let json = serde_json::to_string_pretty(&raw_legis).unwrap();
        // println!("{json}");

        let legis_content = &raw_legis["content"];

        let ityp = legis_content["ityp"].as_str()?.to_string();
        let inr = legis_content["inr"].as_u64()?;
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

        // let votes = extract_votes(legis_content)?;
        let votes = extract_votes(legis_content).unwrap_or_default();
        let topics = extract_topics(legis_content, "topics").unwrap_or_default();
        let eurovoc_topics = extract_topics(legis_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics = extract_topics(legis_content, "headwords").unwrap_or_default();

        let speeches = extract_speeches(legis_content).unwrap_or_default();
        let date = extract_date(legis_content)?;

        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y ").ok()?;

        let accepted = if votes.is_empty() {
            None
        } else {
            legis_content["vote"]["infavor"].as_bool()
        };

        let emphasis = if let Some(shortinfo) = legis_content.get("shortinfo") {
            let teil1 = extract_text_from_html(shortinfo["teil1"].as_str()?);
            let teil2 = extract_text_from_html(shortinfo["teil2"].as_str().unwrap_or_default());
            Some(format!("{teil1} {teil2}"))
        } else {
            None
        };
        
        let created_at = legis_content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;
        
        let updated_at = legis_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let id = format!("{gp}/{ityp}/{inr}");

        Some(LegislativeInitiative {
            title,
            description,
            emphasis,
            inr,
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
            topics,
            eurovoc_topics,
            other_keyword_topics,
            created_at,
            updated_at,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12)
            .ok_or(ScraperError::LegisInitParseError)
    }
}

pub fn extract_date(legis_content: &Value) -> Option<&str> {
    let mut most_fitting_date = None;
    for phase in legis_content.get("phase")?.as_array()? {
        let Some(stages) = phase.get("stages") else {
            continue;
        };

        if !phase["name"].as_str()?.contains("NR") {
            continue;
        }

        // extract_speeches_from_stages(stages, &mut speeches);
        most_fitting_date = extract_most_fitting_date(stages);
    }
    most_fitting_date
}

fn extract_most_fitting_date(raw_stages: &Value) -> Option<&str> {
    let mut fitting_date = None;
    for raw_stage in raw_stages.as_array()? {
        if raw_stage.get("priority").is_none() {
            continue;
        }
        let Some(date) = raw_stage["date"].as_str() else {
            continue;
        };
        fitting_date = Some(date)
    }
    fitting_date
}

fn extract_topics(legis_content: &Value, topics_name: &str) -> Option<Vec<String>> {
    Some(
        legis_content[topics_name]["data"]["bubbles"]
            .as_array()?
            .iter()
            .flat_map(|topic| topic["label"].as_str().map(|topic| topic.to_string()))
            .collect(),
    )
}

fn extract_votes(legis_content: &Value) -> Option<Vec<Vote>> {
    Some(
        legis_content["vote"]["result"]
            .as_array()?
            .iter()
            .flat_map(|vote| {
                Some(Vote {
                    party: Party {
                        name: vote["text"].as_str()?.to_string(),
                        color: vote["color"].as_str()?.to_string(),
                        fraction: vote["fraction"].as_u64()? as u32,
                    },
                    infavor: vote["infavor"].as_bool()?,
                })
            })
            .collect::<Vec<Vote>>(),
    )
}

#[cfg(test)]
mod tests {
    use chrono::NaiveDate;
    use common_scrapes::{Opinion, Party};

    use crate::{
        extract_text_from_html,
        legis_init::{LegislativeInitiatives, Vote},
        AustriaScraper, Scraper,
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
        let paths = scraper.legis_paths(initiatives).unwrap();
        assert!(paths.len() >= 10);
        for path in &paths {
            println!("path: {path:?}");
        }
        println!("paths: {}", paths.len());
    }

    #[tokio::test]
    async fn test_get_legis_init() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            //println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("{}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            println!("topics: {:?}", init.topics);
            // println!("id: {}, title: {}", init.id, init.title);
        }

        println!("elapsed: {:?}", start.elapsed());
    }

    #[tokio::test]
    async fn test_get_legis_init_xxvii_a_3373_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3373")
            .await
            .unwrap();
        println!("init: {init:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_xxvii_i_1774_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1774")
            .await
            .unwrap();
        println!("init: {init:?}");

        assert_eq!(init.date, NaiveDate::from_ymd_opt(2023, 6, 1).unwrap());

        assert_eq!(
            init.description,
            "Strompreiskosten-Ausgleichsgesetz 2022, SAG 2022 (1774 d.B.)"
        );
    }

    #[tokio::test]
    async fn test_get_legis_init_i_1954_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/I/1954")
            .await
            .unwrap();

        assert_eq!(init.topics, vec!["Verkehr und Infrastruktur".to_string()]);

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
                    party: Party {
                        name: "ÖVP".to_string(),
                        color: "#62C3D0".to_string(),
                        fraction: 71,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                    },
                    infavor: true
                }
            ]
        );

        assert!(init.accepted.unwrap());

        assert_eq!(init.id, "XXVII/I/1954");
        assert_eq!(init.ityp, "I".to_string());

        assert_eq!(init.speeches[0].delegate_id, 52687);
        assert_eq!(init.speeches[0].opinion, Opinion::Against);

        assert_eq!(init.speeches[1].delegate_id, 5680);
        assert_eq!(init.speeches[1].opinion, Opinion::Infavor);

        assert_eq!(init.speeches[2].delegate_id, 51557);
        assert_eq!(init.speeches[2].opinion, Opinion::Against);

        assert_eq!(init.speeches[3].delegate_id, 83150);
        assert_eq!(init.speeches[3].opinion, Opinion::Infavor);

        assert_eq!(init.speeches[4].delegate_id, 5653);
        assert_eq!(
            init.speeches[4].opinion,
            Opinion::Other("Regierungsbank".into())
        );
    }

    #[tokio::test]
    async fn test_get_legis_init_a_3021_path() {
        let scraper = AustriaScraper::default();

        let init = scraper
            .get_legis_init("/gegenstand/XXVII/A/3021")
            .await
            .unwrap();

        assert_eq!(init.topics, vec!["Arbeit".to_string()]);

        assert_eq!(init.accepted, Some(true));
        assert_eq!(init.id, "XXVII/A/3021");
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
                    party: Party {
                        name: "ÖVP".to_string(),
                        color: "#62C3D0".to_string(),
                        fraction: 71,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                    },
                    infavor: false
                }
            ]
        );
        let et = init.eurovoc_topics;
        assert_eq!(et, vec!["Arbeitsrecht"]);
        let et = init.other_keyword_topics;
        assert_eq!(et, vec!["Arbeitsrecht I. österreichisches"]);
    }

    #[tokio::test]
    async fn test_xx_legis_init() {
        let mut scraper = AustriaScraper::default();
        scraper.scrape_info.gp = vec!["XX".into()];

        let res = scraper.get_legis_init("/gegenstand/XX/I/1").await.unwrap();
    }

    // test may get invalid -> legis init got voted on potentially
    #[tokio::test]
    async fn test_orf_gesetz_no_vote_at_some_point_xxvii_i_2082() {
        let scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2082")
            .await
            .unwrap();
        println!("res: {res:?}");
        println!("acc: {:?}", res.accepted)
    }
}
