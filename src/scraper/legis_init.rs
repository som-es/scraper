use async_trait::async_trait;
use chrono::{NaiveDate, NaiveDateTime};
use common_scrapes::{AcceptedState, LegislativeInitiative, Party, Speech, Vote, VoteType};
use serde_json::{json, Value};

use crate::{
    documents, error::ScraperError, extract_from_gegenstand_url, extract_pro_contra_party_codes_from_string, extract_text_from_html, extract_text_from_html_via_name_attribute_next_section, find_optimal_votes_by_name, person_url_to_id, proposals::ProposalScraper, remove_steno_info, speeches::extract_speeches, AustriaScraper, DeclineUrlGegenstand, GegenstandPaths, ProContra, Scraper, TextExtractor
};

// const PARTIES: LazyLock<Vec<Party>> = LazyLock::new(|| {
//     let scraper = AustriaScraper::default();
//     pollster::block_on(scraper.get_parties()).unwrap()
// });

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

    async fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative>;

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError>;

    async fn get_legis_init(
        &self,
        proposal_path: &str,
    ) -> Result<LegislativeInitiative, ScraperError> {
        let raw_proposal = self.get_gegenstand_info(proposal_path).await?;
        Self::raw_legis_into_legis(raw_proposal)
            .await
            .ok_or(ScraperError::LegisInitParseError)
    }
}

#[async_trait]
impl LegislativeInitiatives for AustriaScraper {
    const FILTER_PATH: &'static str =
        "/Filter/api/json/post?jsMode=EVAL&FBEZ=WFP_003&listeId=10103&showAll=true";

    fn legis_payload(&self) -> Self::Payload {
        json!({"GP": self.scrape_info.gp, "NRBR": self.scrape_info.council_type})
    }

    async fn raw_legis_into_legis(raw_legis: Value) -> Option<LegislativeInitiative> {
        let description = raw_legis["meta"]["description"].as_str()?.to_string();
        let title = raw_legis["meta"]["title"].as_str()?.to_string();

        // let json = serde_json::to_string_pretty(&raw_legis)?;
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
            .flat_map(|name| name["url"].as_str().map(|url| person_url_to_id(url)))
            .flatten()
            .collect::<Vec<u64>>();

        // let votes = extract_votes(legis_content)?;
        let votes = extract_votes(legis_content).unwrap_or_default();
        let topics = extract_topics(legis_content, "topics").unwrap_or_default();
        let eurovoc_topics = extract_topics(legis_content, "eurovoc").unwrap_or_default();
        let other_keyword_topics = extract_topics(legis_content, "headwords").unwrap_or_default();

        let mut speeches = extract_speeches(legis_content).unwrap_or_default();
        let date = extract_date(legis_content)?;

        let date = NaiveDate::parse_from_str(date, "%d.%m.%Y ").ok()?;

        let mut accepted = if votes.is_empty() {
            None
        } else {
            legis_content["vote"]["infavor"].as_bool()
        }
        .map(|x| {
            if x {
                AcceptedState::Accepted
            } else {
                AcceptedState::Declined
            }
        });

        let emphasis: Option<String> = if let Some(shortinfo) = legis_content.get("shortinfo") {
            let teil1 = extract_text_from_html(shortinfo["teil1"].as_str().unwrap_or_default());
            let teil2 = extract_text_from_html(shortinfo["teil2"].as_str().unwrap_or_default());
            if teil1.is_empty() && teil2.is_empty() {
                None
            } else {
                Some(format!("{teil1} {teil2}"))
            }
        } else {
            None
        };

        let created_at = legis_content["einlangen"].as_str()?;
        let created_at = NaiveDateTime::parse_from_str(created_at, "%Y-%m-%dT%H:%M:%S").ok()?;

        let updated_at = legis_content["update"].as_str();
        let updated_at = updated_at.map(|updated_at| {
            NaiveDateTime::parse_from_str(updated_at, "%Y-%m-%dT%H:%M:%S").unwrap()
        });

        let requires_simple_majority =
            extract_majority_on_indicators(legis_content).unwrap_or(true);
        let (was_invisibly_declined, _) = extract_invisibly_declined(legis_content)?;

        let (vote_type, mut hidden_votes_untranslated, voted_by_name) =
            extract_hidden_votes(legis_content)
                .await
                .unwrap_or_default();
        if accepted.is_none() && was_invisibly_declined {
            accepted = Some(AcceptedState::PreDeclined);

            // proposals are inversed declined warum auch immer !
            match (vote_type, &mut hidden_votes_untranslated) {
                (Some(VoteType::Proposal), Some(pro_contra)) => {
                    core::mem::swap(&mut pro_contra.contra, &mut pro_contra.pro);
                    speeches = speeches
                        .into_iter()
                        .map(|speech| Speech {
                            delegate_id: speech.delegate_id,
                            opinion: speech.opinion.invert(),
                            spoken_text_html_urls: speech.spoken_text_html_urls,
                        })
                        .collect();
                }
                _ => {}
            }
        };

        // if !voted_by_name && eurovoc_topics.contains(&"namentliche Abstimmung".to_string()) {
        //     println!("--> this is interesting")
        // }

        // this also shows stuff for the bundesrat:
        // this may not be enough, go through every stage in
        // let voted_by_name = eurovoc_topics.contains(&"namentliche Abstimmung".to_string());

        let id = format!("{gp}/{ityp}/{inr}");

        let proposal = <Self as ProposalScraper>::raw_proposal_into_proposal(raw_legis)?;

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
            hidden_votes_untranslated,
            pre_declined_type: vote_type,
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
            requires_simple_majority,
            proposal,
            voted_by_name,
        })
    }

    fn legis_paths(&self, initiatives: Value) -> Result<GegenstandPaths, ScraperError> {
        self.gegenstand_paths(initiatives, 12)
            .ok_or(ScraperError::LegisInitParseError)
    }
}

pub fn extract_invisibly_declined(
    legis_content: &Value,
) -> Option<(bool, Option<DeclineUrlGegenstand>)> {
    let status_description = legis_content["status"]["description"].as_str()?;
    if status_description.contains("abgelehnt") {
        return Some((true, extract_from_gegenstand_url(status_description)));
    }

    Some((false, None))
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

pub async fn extract_hidden_votes(
    legis_content: &Value,
) -> Option<(Option<VoteType>, Option<ProContra>, bool)> {
    let mut most_fitting_votes = None;
    for phase in legis_content.get("phase").unwrap().as_array().unwrap() {
        let Some(stages) = phase.get("stages") else {
            continue;
        };

        if phase["name"]
            .as_str()
            .unwrap()
            .contains("Plenarberatungen NR")
        {
            most_fitting_votes = extract_most_fitting_votes(stages).await;
            break;
        }
    }
    most_fitting_votes
}

pub async fn extract_most_fitting_votes(
    stages: &Value,
) -> Option<(Option<VoteType>, Option<ProContra>, bool)> {
    let mut declined_type = None;
    let mut pro_contra = None;
    let mut voted_by_name = false;
    for stage in stages.as_array().unwrap().iter().rev() {
        let Some(text) = stage["text"].as_str() else {
            continue;
        };

        if !(text.contains("Abstimmung")
            || text.contains("abgelehnt")
            || text.contains("angenommen"))
        {
            continue;
        }
        if text.contains("Gesetzesvorschlag in zweiter Lesung") {
            declined_type = Some(VoteType::Second)
        } else if text.contains("Gesetzesvorschlag in dritter Lesung") {
            declined_type = Some(VoteType::Third)
        } else if text.contains("erster Lesung") {
            declined_type = Some(VoteType::First)
        } else if text.contains("Antrag, den ablehnenden Ausschussbericht") {
            declined_type = Some(VoteType::Proposal)
        } else if text.contains("Sitzung des Nationalrates")
            && text.contains("Genehmigung")
            && text.to_lowercase().contains("namentliche")
        {
            declined_type = Some(VoteType::OnlyVotedByName)
        } else {
            // need to find fitting type, some stages are bs
            continue;
        }

        if text.to_lowercase().contains("namentlich") {

            let (vote_sum, pro_sum, contra_sum) = extract_vote_count(&text).unwrap();
            println!("namentlich");
            for protocol_link_obj in stage["fsth"]
                .as_array()
                .cloned()
                .unwrap_or_default()
                .iter()
                .rev()
            {
                let Some(url) = protocol_link_obj["url"].as_str() else {
                    continue;
                };

                // let start_site = protocol_link_obj["fund_von"].as_u64();
                // let end_site = protocol_link_obj["fund_bis"].as_u64();
                // match (start_site, end_site) {
                //     (Some(start_site), Some(end_site)) => {
                //         // ahhh
                //     }
                //     (None, None) => {
                //         // fine
                //     }
                //     _ => panic!("should not be possible hopefully"),
                // }
                let mut extractor = TextExtractor::new(AustriaScraper::BASE_URL);
                let Some(html_text) = extractor
                    .extract_text_from_url(&url, Some(find_optimal_votes_by_name))
                    .await
                else {
                    continue;
                };
                println!("fin");
                // extract_votes_by_name(&remove_steno_info(&html_text));

                if true {
                    break;
                }
            }
            voted_by_name = true;
            println!("aus namentlich");
            break;
        }


        pro_contra = extract_pro_contra_party_codes_from_string(text);
        if pro_contra.is_some() {
            break;
        }
    }

    Some((declined_type, pro_contra, voted_by_name))
}

fn clear_nr_info() {}

fn extract_vote_count(text: &str) -> Option<(usize, usize, usize)> {
    let text = text.to_string().replace(';', ",");
    println!("text: {text}");
    let start = text.find("abgegebene Stimmen:").unwrap();
    let vote_info = &text[start..];
    let vote_info = &vote_info[vote_info.find(':').unwrap() + 1..vote_info.find(')').unwrap()];
    println!("vote_info: {vote_info}");
    let end_sum = vote_info.find(',').unwrap();

    let vote_sum = vote_info[1..end_sum].parse::<usize>().unwrap();

    let shorter_info = &vote_info[end_sum+1..];
    
    let pro = shorter_info[shorter_info.find(':').unwrap() + 2..shorter_info.find(',').unwrap()]
        .parse::<usize>()
        .unwrap();

    let shorter_info = &shorter_info[shorter_info.find(',').unwrap() + 1..];
    let contra = shorter_info[shorter_info.find(':').unwrap() + 2..]
        .parse::<usize>()
        .unwrap();
    assert_eq!(vote_sum, pro + contra);
    Some((vote_sum, pro, contra))
}

fn extract_votes_by_name(from_html_text: &str) -> Option<ProContra> {
    // println!("from_html_texxt: {from_html_text}");

    let find_str = "Mit „Ja“ stimmten";
    let pro_votes_start_idx = from_html_text.find(find_str).unwrap();

    let pro_open_ended = &from_html_text[pro_votes_start_idx..];
    let pro_open_ended = &pro_open_ended[pro_open_ended.find(':')? + 1..];
    let end_pro_idx = pro_open_ended.find('.').unwrap();
    let pro_delegates = &pro_open_ended[..end_pro_idx];
    println!("------------ pro_delegates: {pro_delegates}");
    // println!("--------------- text: {from_html_text}");

    let find_str = "Mit „Nein“ stimmten";
    let contra_votes_start_idx = pro_open_ended.find(find_str).unwrap();

    let contra_open_ended = &pro_open_ended[contra_votes_start_idx..];
    let contra_open_ended = &contra_open_ended[contra_open_ended.find(':')? + 1..];
    let end_contra_idx = contra_open_ended.find('.').unwrap();
    let contra_delegates = &contra_open_ended[..end_contra_idx];

    println!("-------------------- contra_delegates: {contra_delegates}");
    None
}

fn extract_majority_on_indicators(legis_content: &Value) -> Option<bool> {
    let mut is_simple_majority = true;

    'outer: for phase in legis_content.get("phase")?.as_array()?.iter() {
        let Some(stages) = phase.get("stages") else {
            continue;
        };
        for stage in stages.as_array()? {
            if let Some(text) = stage.get("text") {
                if text.as_str()?.contains("Mehrheit von zwei Dritteln")
                    || text.as_str()?.contains("Bundesverfassungsgesetz")
                {
                    is_simple_majority = false;
                    break 'outer;
                }
            }
        }
    }

    if legis_content["status"]["description"]
        .as_str()?
        .contains("nicht die erforderliche 2/3 Mehrheit")
    {
        is_simple_majority = false
    }

    Some(is_simple_majority)
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

pub fn extract_topics(legis_content: &Value, topics_name: &str) -> Option<Vec<String>> {
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
                        code: vote["code"].as_str()?.to_string(),
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
    use common_scrapes::{AcceptedState, Opinion, Party, VoteType};

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

    // /gegenstand/XXVII/A/4129

    #[tokio::test]
    async fn test_get_legis_init_invisibly_declined_2_3_majority() {
        let scraper = AustriaScraper::default();
        let legis_init = scraper
            .get_legis_init("/gegenstand/XXVII/A/4129")
            .await
            .unwrap();
        assert_eq!(legis_init.delegate_ids, vec![5439, 87001]);
        assert!(!legis_init.requires_simple_majority);
        assert_eq!(legis_init.accepted.unwrap(), AcceptedState::PreDeclined);
        assert!(legis_init.speeches.len() > 0);
        let party_codes = legis_init.hidden_votes_untranslated.unwrap();
        assert_eq!(party_codes.pro, vec!["V".to_string(), "G".to_string()]);
        assert_eq!(
            party_codes.contra,
            vec!["S".to_string(), "F".to_string(), "N".to_string()]
        );
        assert_eq!(legis_init.pre_declined_type.unwrap(), VoteType::Second);
    }

    #[tokio::test]
    async fn test_get_legis_init_was_voted_by_name() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        let mut count = 0;
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("error {}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            if init.voted_by_name {
                println!(" {}{path}", AustriaScraper::BASE_URL,);
                count += 1;
            }
        }

        println!("elapsed: {:?}", start.elapsed());
        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_was_invisibly_declined() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        let mut count = 0;
        for path in scraper.legis_paths(initiatives).unwrap() {
            println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("error {}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            if init.accepted == Some(AcceptedState::PreDeclined) {
                println!(
                    "{}, {hidden:?}, {ty:?} was_invisibly_declined: {:?} {}{path}",
                    init.voted_by_name,
                    init.accepted,
                    AustriaScraper::BASE_URL,
                    hidden = init.hidden_votes_untranslated,
                    ty = init.pre_declined_type
                );
                count += 1;
            }
        }

        println!("elapsed: {:?}", start.elapsed());
        println!("count: {count:?}");
    }

    #[tokio::test]
    async fn test_get_legis_init_majority() {
        let scraper = AustriaScraper::default();

        let start = std::time::Instant::now();

        let initiatives = scraper.post_legis_payload().await.unwrap();
        for path in scraper.legis_paths(initiatives).unwrap() {
            //println!("path: {path:?}");
            let Ok(init) = scraper.get_legis_init(&path).await else {
                println!("error {}{path}", AustriaScraper::BASE_URL);
                continue;
            };
            // if init.accepted == Some(false) {
            // println!("{:?} 1/2 majority?: {} {}{path}", init.accepted,init.requires_simple_majority, AustriaScraper::BASE_URL);
            if !init.requires_simple_majority {
                println!("---------------------------------");
            }
            println!(
                "1/2 majority?: {} {}{path}",
                init.requires_simple_majority,
                AustriaScraper::BASE_URL
            );
            if !init.requires_simple_majority {
                println!("---------------------------------");
            }
            // }
            // println!("{:?} {}{path}", init.accepted, AustriaScraper::BASE_URL);
            // println!("topics: {:?}", init.topics);
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
                        code: "V".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                        code: "S".to_string(),
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                        code: "F".to_string(),
                    },
                    infavor: false
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                        code: "G".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                        code: "N".to_string(),
                    },
                    infavor: true
                }
            ]
        );

        assert_eq!(init.accepted.unwrap(), AcceptedState::Accepted);

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

        assert_eq!(init.accepted, Some(AcceptedState::Accepted));
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
                        code: "V".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "SPÖ".to_string(),
                        color: "#FF0000".to_string(),
                        fraction: 40,
                        code: "S".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "FPÖ".to_string(),
                        color: "#0052FB".to_string(),
                        fraction: 30,
                        code: "F".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "GRÜNE".to_string(),
                        color: "#69B12E".to_string(),
                        fraction: 26,
                        code: "G".to_string(),
                    },
                    infavor: true
                },
                Vote {
                    party: Party {
                        name: "NEOS".to_string(),
                        color: "#E3257B".to_string(),
                        fraction: 15,
                        code: "N".to_string(),
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

    // /gegenstand/XXVII/A/3945
    #[tokio::test]
    async fn test_legis_init_voted_by_name_xxvii_a_3945() {
        let mut scraper = AustriaScraper::default();
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/A/3945")
            .await
            .unwrap();
    }
    #[tokio::test]
    async fn test_xxvii_i_2269_legis_init_was_invisibly_declined_false() {
        let mut scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2269")
            .await
            .unwrap();
        assert_ne!(res.accepted, Some(AcceptedState::PreDeclined));
    }

    #[tokio::test]
    async fn test_xxvii_erroring_legis_init_i_405() {
        //https://www.parlament.gv.at/gegenstand/XXVII/I/405
        let scraper = AustriaScraper::default();

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/405")
            .await
            .unwrap();
    }

    // test may get invalid -> legis init got voted on potentially
    #[tokio::test]
    async fn test_orf_gesetz_no_vote_at_some_point_xxvii_i_2082() {
        let scraper = AustriaScraper::default();

        // let out = scraper.get_gegenstand_info("/gegenstand/XXVII/I/2082").await.unwrap();
        let out = scraper
            .get_gegenstand_info("/gegenstand/XXVII/A/4070")
            .await
            .unwrap();
        println!("{}", serde_json::to_string_pretty(&out).unwrap());
        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2082")
            .await
            .unwrap();

        println!("res: {res:?}");
        println!("acc: {:?}", res.accepted);

        let res = scraper
            .get_legis_init("/gegenstand/XXVII/I/2269")
            .await
            .unwrap();

        println!("acc: {:?}", res.accepted);

        // gegenstand/XXVII/I/2269
    }
}
