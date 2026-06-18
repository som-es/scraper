use crate::plenar::{Debate, DebateInfo};
use common_scrapes::{Opinion, PlenarSpeech, Reference, Speech};
use serde_json::Value;
use std::collections::BTreeMap;

pub(super) fn combine_speech_info(
    debates_with_speeches: Vec<(DebateInfo, Vec<PlenarSpeech>)>,
    document_link_speeches: Vec<Vec<Speech>>,
) -> Vec<Debate> {
    let document_link_speeches = document_link_speeches
        .into_iter()
        .map(|speeches| {
            (
                speeches
                    .iter()
                    .map(|speech| speech.delegate_id)
                    .collect::<Vec<_>>(),
                speeches,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let debates_with_speeches_keys = debates_with_speeches
        .into_iter()
        .map(|(about, speeches)| {
            (
                speeches
                    .iter()
                    .map(|speech| speech.speech.delegate_id)
                    .collect::<Vec<_>>(),
                about,
                speeches,
            )
        })
        .collect::<Vec<_>>();

    let mut debates = vec![];
    for (debates_key, debate_info, mut debate_speeches) in debates_with_speeches_keys {
        let document_link_speeches = document_link_speeches
            .get(&debates_key)
            .map(|speeches| speeches.as_slice())
            .unwrap_or_default();
        if document_link_speeches.is_empty() {
            log::warn!(
                "Keine document links! Aber {} speeches",
                debate_speeches.len()
            );
        }
        for (debate_speech, document_link_speech) in
            debate_speeches.iter_mut().zip(document_link_speeches)
        {
            debate_speech.speech = document_link_speech.clone();
        }
        debates.push(Debate {
            info: debate_info,
            speeches: debate_speeches,
        });
    }
    debates
}

pub(super) fn extract_speeches(past_debates: &[Value]) -> Vec<(DebateInfo, Vec<PlenarSpeech>)> {
    past_debates
        .iter()
        .flat_map(|past_debate| {
            Some((
                past_debate["text"].as_str()?,
                past_debate["speeches"].as_array()?,
                past_debate["agenda"].as_array()?,
            ))
        })
        .filter(|(_, speeches, _)| !speeches.is_empty())
        .flat_map(|(text, speeches, refs)| {
            let references = refs
                .iter()
                .flat_map(|x| {
                    Some(Reference {
                        gp: x["gp_code"].as_str()?.to_string(),
                        ityp: x["ityp"].as_str()?.to_string(),
                        inr: x["inr"].as_i64()? as u32,
                    })
                })
                .collect::<Vec<_>>();
            Some((
                DebateInfo {
                    about: text.to_string(),
                    references,
                },
                speeches
                    .iter()
                    .flat_map(|speech| {
                        let delegate_id = speech[3].as_u64()?;
                        let ty = speech[5].as_str()?.to_string();
                        let duration_in_seconds = speech[7]
                            .as_str()
                            .map(|duration| {
                                let duration = duration.split(':').collect::<Vec<&str>>();

                                Some(
                                    duration[0].parse::<u32>().ok()? * 60
                                        + duration[1].parse::<u32>().ok()?,
                                )
                            })
                            .flatten();

                        Some(PlenarSpeech {
                            speech: Speech {
                                delegate_id,
                                delegate_display_name: String::new(),
                                opinion: Opinion::None,
                                spoken_text_html_urls: vec![],
                            },
                            ty,
                            duration_in_seconds,
                        })
                    })
                    .collect::<Vec<_>>(),
            ))
        })
        .collect::<Vec<_>>()
}

#[cfg(test)]
mod tests {
    use common_scrapes::{
        Opinion::{Against, Infavor, Other},
        PlenarSpeech, Speech, SpeechTextInfo,
    };

    use crate::TextExtractor;

    #[tokio::test]
    async fn test_edith_muehlberghuber_speech() {
        let speech = Speech {
            delegate_id: 51569,
            delegate_display_name: "Edith Mühlberghuber (F)".into(),
            opinion: Against,
            spoken_text_html_urls: vec![
                SpeechTextInfo {
                    site_start: Some(
                        264,
                    ),
                    site_end: Some(
                        265,
                    ),
                    document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/117/fnameorig_1426448.html#RU_250908".into(),
                },
            ],
        };

        let mut extractor = TextExtractor::new();
        let unfitted_speech = extractor
            .raw_speech_extraction(&speech.spoken_text_html_urls[0])
            .await
            .unwrap();

        dbg!(&unfitted_speech);
        assert!(unfitted_speech.contains("che Auswirkungen eine Frauenquote hat und welche "));
        assert!(unfitted_speech.len() < 3000);
    }

    #[tokio::test]
    async fn test_title_back_speech_fit() {
        let speech = "Bundesminister für Land- und Forstwirtschaft, Klima- und Umweltschutz, Regionen und Wasserwirtschaft Mag. Norbert Totschnig, MSc: Vielen Dank, sehr geehrter Herr Präsident! Geschätzte Damen und Herren Nationalrätinnen und Nationalräte! Hohes Haus! Ich möchte mein Statement erstens in die UG 42: Landwirtschaft, Forstwirtschaft und Wasserwirtschaft, und zweitens in die UG 43: Umwelt, Klimawandelanpassung und Kreislaufwirtschaft, unterteilen.

        Zur Landwirtschaft: Mit dem Budget für die Land- und Forstwirtschaft, Wasserwirtschaft und Regionen setzen wir ein klares Zeichen für unsere bäuerlichen Familienbetriebe, für intakte Regionen, für eine nachhaltige Ressourcennutzung. Das ist ein klares Statement für eine ökosoziale Verantwortung, die wir seit vielen Jahren in diesem Land leben. Es stehen rund 3 Milliarden Euro zur Verfügung, genau sind es 2,84 Milliarden Euro für 2025 und 2,88 Milliarden Euro für 2026. Damit sichern wir die Versorgung im Land, damit sichern wir die Lebensqualität im ländlichen Raum.

        Herr Kollege Schmiedlechner, ich weiß nicht, über welches Budget du geredet hast, es waren jedenfalls nicht das Budget 2025 und das Budget 2026. (Abg. Reiter [ÖVP]: Wahrscheinlich hat er es nicht gelesen!) Natürlich sparen wir auch im Bereich der Landwirtschaft ein. Wir sparen in der Verwaltung ein. Das führt dazu, dass wir in Zukunft auch mehr an Effizienz brauchen. Wir müssen KI einsetzen, es werden da und dort auch keine Posten nachbesetzt. Da sparen wir insgesamt 16 Millionen Euro ein. Wenn wir alles zusammenrechnen, inklusive der Mittel für die Förderungen, technische Anpassungen im Bereich der Gemeinsamen Agrarpolitik, in der Landwirtschaft, Forstwirtschaft und beim Wald – vorhin schon angesprochen –, dann reden wir von 125 Millionen Euro in beiden Jahren, bei einem Budget von insgesamt fast 6 Milliarden Euro. Also wir sparen, aber es ist noch genügend Geld da, vor allem in den Bereichen, in denen wir Wert darauf legen, dass die Finanzierung auch weiterhin gesichert ist. (Beifall bei der ÖVP.)";

        let name = "Norbert Totschnig, MSc (ÖVP)";

        let mut text_extractor = TextExtractor::new();
        let fitted_speech = text_extractor.fit_speech(speech.to_string(), name).await;
        let truth_fitted_speech = "Norbert Totschnig, MSc: Vielen Dank, sehr geehrter Herr Präsident! Geschätzte Damen und Herren Nationalrätinnen und Nationalräte! Hohes Haus! Ich möchte mein Statement erstens in die UG 42: Landwirtschaft, Forstwirtschaft und Wasserwirtschaft, und zweitens in die UG 43: Umwelt, Klimawandelanpassung und Kreislaufwirtschaft, unterteilen.\n        Zur Landwirtschaft: Mit dem Budget für die Land- und Forstwirtschaft, Wasserwirtschaft und Regionen setzen wir ein klares Zeichen für unsere bäuerlichen Familienbetriebe, für intakte Regionen, für eine nachhaltige Ressourcennutzung. Das ist ein klares Statement für eine ökosoziale Verantwortung, die wir seit vielen Jahren in diesem Land leben. Es stehen rund 3 Milliarden Euro zur Verfügung, genau sind es 2,84 Milliarden Euro für 2025 und 2,88 Milliarden Euro für 2026. Damit sichern wir die Versorgung im Land, damit sichern wir die Lebensqualität im ländlichen Raum.\n        Herr Kollege Schmiedlechner, ich weiß nicht, über welches Budget du geredet hast, es waren jedenfalls nicht das Budget 2025 und das Budget 2026. (Abg. Reiter [ÖVP]: Wahrscheinlich hat er es nicht gelesen!) Natürlich sparen wir auch im Bereich der Landwirtschaft ein. Wir sparen in der Verwaltung ein. Das führt dazu, dass wir in Zukunft auch mehr an Effizienz brauchen. Wir müssen KI einsetzen, es werden da und dort auch keine Posten nachbesetzt. Da sparen wir insgesamt 16 Millionen Euro ein. Wenn wir alles zusammenrechnen, inklusive der Mittel für die Förderungen, technische Anpassungen im Bereich der Gemeinsamen Agrarpolitik, in der Landwirtschaft, Forstwirtschaft und beim Wald – vorhin schon angesprochen –, dann reden wir von 125 Millionen Euro in beiden Jahren, bei einem Budget von insgesamt fast 6 Milliarden Euro. Also wir sparen, aber es ist noch genügend Geld da, vor allem in den Bereichen, in denen wir Wert darauf legen, dass die Finanzierung auch weiterhin gesichert ist. (Beifall bei der ÖVP.)";

        assert_eq!(truth_fitted_speech, fitted_speech.unwrap());
    }

    #[tokio::test]
    async fn test_speech_fit_bmff_susanne_raab() {
        let speech =  PlenarSpeech {
           ty: "rb".into(),
           duration_in_seconds: Some(
               242,
           ),
           speech: Speech {
               delegate_id: 6486,
               delegate_display_name: "BM MMag. Dr. Susanne Raab (V)".into(),
               opinion: Other(
                   "Regierungsbank".into(),
               ),
               spoken_text_html_urls: vec![
                   SpeechTextInfo {
                       site_start: Some(
                           281,
                       ),
                       site_end: Some(
                           283,
                       ),
                       document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/241/fnameorig_1629737.html#RU_310851".into(),
                   },
               ],
           },
       };

        let mut extractor = TextExtractor::new();

        let unfitted_speech = extractor
            .raw_speech_extraction(&speech.speech.spoken_text_html_urls[0])
            .await
            .unwrap();
        dbg!(&unfitted_speech);
        let text = extractor
            .speech_extraction_with_fit(
                &speech.speech.spoken_text_html_urls[0],
                &speech.speech.delegate_display_name,
            )
            .await;

        let truth = "Susanne Raab:\nSehr geehrter Herr Präsident! Sehr geehrte Damen und Herren Abgeordnete!\nTatsächlich, und das haben die Vorrednerinnen und Vorredner angesprochen,\nhat sich die Situation für Medien in ganz Europa in den letzten Jahren verändert.\nDas ist natürlich primär aufgrund der Digitalisierung passiert,\naufgrund des Abflusses von Werbegeldern an internationale Konzerne und\nschlichtweg aufgrund der Tatsache, dass 2015 noch fast jeder Dritte\ntäglich ein Printerzeugnis erworben hat – mittlerweile ist es\nnur mehr fast jeder Fünfte.\nWir wollen eine\nvielfältige Medienlandschaft; wir wollen das, weil wir davon\nüberzeugt sind, dass Qualitätsjournalismus einen Mehrwert";

        assert!(text.as_ref().unwrap().len() < 8000);
        assert!(text.unwrap().contains(truth));
    }

    #[tokio::test]
    async fn test_speech_extraction_matznetter() {
        let speech =  Speech {
            delegate_id: 14844,
            delegate_display_name: "Dr. Christoph Matznetter (S)".into(),
            opinion: Against,
            spoken_text_html_urls: vec![
                SpeechTextInfo {
                    site_start: Some(
                        157,
                    ),
                    site_end: Some(
                        158,
                    ),
                    document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/62/fnameorig_884816.html#RU_228345".into(),
                },
            ],
        };

        let mut extractor = TextExtractor::new();

        let speech = extractor
            .speech_extraction_with_fit(
                &speech.spoken_text_html_urls[0],
                &speech.delegate_display_name,
            )
            .await
            .unwrap();

        println!("speech: {speech:?}");
        assert_eq!(speech, "Christoph Matznetter (SPÖ):\nHerr Präsident! Meine Damen und Herren auf der Regierungsbank! Wieder eine\nneue Folge der beliebten Serie „Pleiten-, Pech- und Pannendienst“,\nder Hauptdarsteller Gernot Blümel ist erst jetzt vor wenigen Sekunden\nzurückgekommen. Das wundert mich nicht: Der Herr Bundeskanzler hat es ja überhaupt\nvorgezogen, nicht teilzunehmen. Ich verstehe das, nach diesen Zuständen,\ndie wir schon im Frühjahr hatten.\nAbgeordneter Krainer hat zu Recht daran erinnert, dass hier\nein Beschluss zum Budget mit 102 000 Euro statt 102 Milliarden\nEuro gefasst worden wäre, hätte er nicht nach der zweiten Lesung die\nHelping Hands angeboten und Ihnen den Ausweg gezeigt, das über Nacht\nauszubessern – und jetzt liefert unser Hauptdarsteller beim\n„Pleiten-, Pech- und Pannendienst“ erneut ein Budget, dessen Zahlen\nnichts mit der Wirklichkeit zu tun haben. Die Hilfestellungen, die wir\nangeboten haben – Rückverweis, überarbeiten, den nächsten\nLockdown berücksichtigen –, werden schnöde abgewiesen.\nNein, am ersten Tag des harten Lockdowns versammeln sich 183 plus\n20 – nein, der Bundeskanzler ist weggeblieben – plus\nPersonal hier, um ein Budget zu diskutieren, das Makulatur ist. Was soll das, meine Damen und Herren? (Zwischenruf\ndes Abg. Deimek. – Abg. Hafenecker: Keine\nWahrnehmung ...!)\nGleichzeitig sind dieselben Regierungsfraktionen, die das\ndurchziehen, nicht bereit, bei ihren Maßnahmen Sorgfalt an den Tag zu\nlegen. Gleichzeitig gibt es für einen Teil der Branchen 80 Prozent\nUmsatzentschädigung ohne Anrechnung von Kurzarbeit – super\nfür die Betroffenen und vielleicht für den einen Monat ein bisschen\neine Entschädigung dafür, dass Sie bis heute vergessen haben,\nfür den Lockdown eins im Frühjahr den Großteil auszubezahlen.\nEs sind ja beim Härtefallfonds erst 700 Millionen Euro von\n2 Milliarden Euro, Herr Kollege Schwarz, und bei den Fixkosten\n252 Millionen Euro statt Milliarden ausgezahlt worden. Sie haben sie nicht\nentschädigt, und jetzt versuchen Sie, einen Teil mit 80 Prozent zu\nködern. Da passiert es ja wiederum: Die\nGlücksspielhöllen – so hat man früher gesagt, in\nmeiner Jugend –, die wir in Wien schon lange verboten haben, kriegen\n80 Prozent vom Umsatz ersetzt, Herr Finanzminister?! (Ruf: Nein!) Die Zulieferer für die Weihnachtsmärkte, die null Umsatz\nhaben werden, schauen aber durch die Finger. Sie können es einfach nicht,\nmeine Damen und Herren! (Beifall bei der SPÖ sowie bei Abgeordneten von\nFPÖ und NEOS.)\nEs gibt zwei Möglichkeiten: Treten Sie\nzurück und schauen Sie, dass andere kommen, oder versuchen Sie, eine\nLernkurve zu haben, die ein bisschen steiler ist als bisher – ein\nbisschen! Versuchen Sie, das nächste Mal etwas vorzulegen, das passt! (Zwischenruf\ndes Abg. Hafenecker.) Bitte, Herr Blümel, das werden Sie doch\nhoffentlich früher oder später hinkriegen, denn wenn Sie jetzt\nfünf Jahre so weitermachen, müssen wir keine Budgets mehr\nbeschließen: Ist eh wurscht, was drinsteht; schreiben wir ein paar Nullen\nrein oder nicht! – Ich fürchte, die Serie „Pleiten‑,\nPech- und Pannendienst“ wird noch eine Weile lang so weitergehen, meine\nDamen und Herren. – Danke. (Beifall bei der\nSPÖ. – Abg. Haubner: Furchtbar!)\n");
    }

    #[tokio::test]
    async fn test_speech_extraction_gruenber() {
        let speech = Speech {
            delegate_id: 1979,
            delegate_display_name: "Kira Grünberg (V)".into(),
            opinion: Infavor,
            spoken_text_html_urls: vec![
                SpeechTextInfo {
                    site_start: Some(
                        189,
                    ),
                    site_end: Some(
                        191,
                    ),
                    document_url: "https://www.parlament.gv.at/dokument/XXVI/NRSITZ/43/fnameorig_753794.html#RU_189133".into(),
                },
            ],
        };

        let mut extractor = TextExtractor::new();
        let text = extractor
            .full_speech_extraction(&speech.spoken_text_html_urls, &speech.delegate_display_name)
            .await
            .unwrap();
        println!("text: {text:?}");
        assert_eq!(text, "Kira Grünberg (ÖVP): Sehr geehrte Frau Präsidentin! Geschätzte Frau Minister!\nGeschätzter Herr Minister! Liebe Kolleginnen und Kollegen! Liebe Zuseherin-\nnen\nund Zuseher! Meine Vorrednerinnen und Vorredner haben bereits ausgeführt,\nwarum es in den letzten Wochen in Einzelfällen bedauerlicherweise zum\nAuszahlungsstopp der erhöhten Familienbeihilfe gekommen ist.\nZum Glück ist unsere Bundesministerin\nBogner-Strauß sofort eingesprungen und hat klargestellt, dass es da zu\nkeiner Schlechterstellung kommt und dass die erhöhte Familienbeihilfe\nfür Kinder und Erwachsene mit Behinderung weiterhin ausbezahlt wird. Auch\nRückforderungen können damit ausgeschlossen werden.\nMit dem vorliegenden Initiativantrag wird\ndiese Zusicherung nun gesetzlich festgehalten. Ergänzend dazu bringe\nich folgenden Entschließungsantrag ein:\nEntschließungsantrag \nder Abgeordneten Norbert Sieber, Edith\nMühlberghuber, Birgit Silvia Sandler, Michael Bernhard, Daniela\nHolzinger-Vogtenhuber, BA, Kolleginnen und Kollegen betreffend\n„Einbeziehung von Behindertenorganisationen in die Erarbeitung des\nEinführungserlasses und Evaluierung der Neuregelung“\nEntschließungsantrag\nDer Nationalrat wolle beschließen:\n„Die Bundesministerin für Frauen,\nFamilie und Jugend wird ersucht,\n– Behindertenorganisationen in die\nErarbeitung des an die Finanzämter adressierten Einführungserlasses\nzur Vollziehung der gegenständlichen Sicherstellung der erhöhten\nFamilienbeihilfe für alle Menschen mit Behinderung, die bisher einen\nEigenanspruch hatten, einzubeziehen und\n– die Vollziehung dieser Bestimmungen auf\nBasis eines laufenden Monitorings im Hinblick auf Einzelfälle und\nGesamtvolumen ein Jahr nach Inkrafttreten einer Evaluierung zu unterziehen und\ndiese in Form eines Berichts dem Parlament zuzuleiten.\n*****\nDieser Entschließungsantrag, der von allen\nFraktionen mitgetragen wird, ist in einem konstruktiven Gespräch mit\nVertreterinnen und Vertretern von Behindertenorganisationen entstanden.\nAuch wenn in diesem Gespräch noch nicht alle Bedenken ausgeräumt\nwerden konnten, so hoffe ich doch sehr, dass dies im weiteren Verlauf passiert.\nIch bin davon überzeugt, dass der\nInitiativantrag die bisherige bewährte Praxis sicherstellt. Und wenn\nIhnen zu Ohren kommt, dass es einen Fall eines Menschen mit Behinderung gibt,\nder Anspruch auf erhöhte Familienbeihilfe hatte und diesen jetzt nicht\nmehr hat, dann melden Sie sich bei uns! Das Ministerium wird jeden Fall einzeln\nprüfen und sich ihn noch einmal genau anschauen. (Beifall bei ÖVP\nund FPÖ.)\nIch möchte hier aber noch eine ganz andere\nPerspektive ansprechen. Es steht außer Frage, dass ein\nbehinderungsbedingter Mehraufwand von der öffentlichen Hand aufgefangen\nwerden soll. Was ich allerdings schon infrage stelle, ist, dass erwachsene Menschen\nmit Behinderung Kinderbeihilfe, wie wir sie in der Umgangssprache nennen, zuerkannt\nbekommen. Das Gesetz macht damit Erwachsene, selbstbewusste und selbstbestimmte\nMenschen zu Kindern.\nManche mögen jetzt sagen, das ist\nWortklauberei oder das ist überhaupt nicht wichtig, aber ich finde das sehr\nwichtig, denn wie sagt so schön der Talmud: Achte auf deine \nGedanken, denn\nsie werden Worte. Achte auf deine Worte, denn sie werden Handlungen. –\nZitatende.\nWenn wir erwachsene Menschen mit\nBehinderungen als Kinder denken, behandeln wir sie vermutlich dann auch so.\nKein erwachsener Mann und keine erwachsene Frau möchte wie ein Kind\nbehandelt werden. Tun wir das bitte auch nicht mit Menschen mit\nBehinderung! – Vielen Dank. (Anhaltender Beifall bei ÖVP und\nFPÖ sowie Beifall bei Abgeordneten der SPÖ.) \n");
    }

    #[tokio::test]
    async fn test_speech_extraction_with_multiple_doc_links() {
        let speech = Speech {
            delegate_id: 35908,
            delegate_display_name: "Katharina Kucharowits (S)".into(),
            opinion: Against,
            spoken_text_html_urls: vec![
                SpeechTextInfo {
                    site_start: Some(
                        105,
                    ),
                    site_end: Some(
                        106,
                    ),
                    document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/117/fnameorig_1426448.html#RU_250358".into(),
                },
                SpeechTextInfo {
                    site_start: Some(
                        106,
                    ),
                    site_end: None,
                    document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/117/fnameorig_1426448.html#RU_250431".into(),
                },
            ],
        };

        let mut extractor = TextExtractor::new();
        let text = extractor
            .full_speech_extraction(&speech.spoken_text_html_urls, &speech.delegate_display_name)
            .await
            .unwrap();

        assert_eq!(text, "Katharina Kucharowits (SPÖ): Herr Präsident! Werter Herr\nBundesminister! Geschätzte Kollegen und Kolleginnen! Liebe Zuseherinnen\nund Zuseher! Sie haben schon gehört, dass wir jetzt sozusagen etliches auf\nder Tagesordnung haben: Es geht um ein Sammelgesetz, in dem es um neue Titel\ngeht – Kollegin Kuntzl ist im Detail darauf eingegangen –,\nin dem es um die Weiterführung von Zugangsbeschränkungen geht, in dem\nes aber auch um QuereinsteigerInnen für pädagogische Berufe\ngeht – und außerdem geht es eben um zwei Anträge, die\nvonseiten der Sozialdemokratie eingebracht wurden, und denen möchte\nich mich jetzt gern widmen.\nDer eine betrifft den Brexit. Sie wissen,\ndiese Entscheidung fiel eigentlich vor vielen Jahren, und auch\nheute – vor allem auch heute – sind wir immer noch mit\nden Folgen konfrontiert, zum einen klarerweise auf einer übergeordneten\nEbene – denken wir an den Handel, denken wir an Jobsituationen,\ndenken wir an Aus- und Einreise –,der Austritt hat aber auch sehr,\nsehr individuelle Auswirkungen, auch im Bildungsbereich. \nSie wissen, Großbritannien ist ein sehr,\nsehr beliebtes Land für SchülerInnenaustausch, Studierendenaustausch,\naber auch Lehrlingsaustausch, ob für Sprachaufenthalte, Uniaufenthalte\noder Ausbildungsaufenthalte. Erasmus und Erasmus plus haben es in den\nletzten Jahrzehnten, möchte ich sagen, vielen Jugendlichen\nermöglicht, diesen geförderten Austausch auch wirklich zu leben.\nNun ist der geförderte Austausch vorbei,\ndiese Möglichkeit ist weg. Und wer soll sich das künftig noch leisten\nkönnen? – Es ist vermutlich eine sehr, sehr kleine Gruppe von\nJugendlichen, und ich finde es halt einfach sehr schade, dass diese Chancen\nallen Jugendlichen genommen werden. Deshalb haben Kollegin Holzleitner  –\nund die Sozialdemokratie –\nden Antrag gestellt, um eben eine Lösung auf die Beine zu stellen.\nIn diesem Antrag haben wir Sie, Herr\nBundesminister, aufgefordert, zum einen mit Kollegen und Kolleginnen in\nder österreichischen Bundesregierung, vor allem aber auf europäischer\nEbene Gespräche zu führen und einfach Lösungen auf die Beine zu\nstellen – es gibt ja auch mit anderen Ländern bilaterale\nAbkommen: Ich denke an die Türkei oder an Norwegen. Das heißt, diese\nOptionen gibt es, und ich finde es deshalb sehr, sehr \nschade, dass die ÖVP\nund die Grünen diesem Antrag ganz einfach nicht zugestimmt haben. Es ist\nsehr, sehr traurig für viele Jugendliche. (Beifall bei der SPÖ.)\nAuch interessant ist, dass Sie dem Antrag zur\nDigitalisierung von Hochschulen Ihre Zustimmung nicht gegeben haben.\nFrau Kollegin Blimlinger, ich würde Sie\nbitten, Ihre Gestik einfach ein bisschen einzustellen. Ich habe gerade\ngesehen, dass Sie den Vogel gedeutet haben, und ich halte das\nsozusagen - - (Beifall bei der SPÖ. – Abg. Blimlinger\ntippt sich an die Schläfe.)\n13.32.48*****\nPräsident\nIng. Norbert Hofer: Frau Abgeordnete Blimlinger,\nich erteile Ihnen für diese Geste einen Ordnungsruf. (Beifall bei SPÖ und FPÖ.)\n*****\nBitte, Frau Abgeordnete. \nKatharina Kucharowits (fortsetzend):\nDanke. – Zurück zur Digitalisierung von Hochschulen: Wir\nhaben, glaube ich, seit Covid-19 bemerkt, wo es bei der\nDigitalisierung hinkt und wo wir in Österreich vor allem auch nachhinken.\nDas eine ist die Ebene der Infrastruktur – vom Recht auf Internet\nfür alle sind wir noch sehr weit entfernt; denken wir an den noch nicht so\nweit fortgeschrittenen Breitbandausbau –, wir haben aber im Rahmen\nder Digitalisierung der Lehre auch bemerkt, dass es da großen\nAufholbedarf an den Universitäten und Hochschulen gibt. Das liegt bitte\nnicht an den Lehrenden, das möchte ich an dieser Stelle dezidiert sagen,\nes liegt ganz einfach an den zu geringen Ressourcen, und ich hätte, wir\nhätten uns da ganz einfach auch von Ihnen, Herr Wissenschaftsminister,\neine extra Finanzspritze, zweckgewidmet für den Ausbau der\nDigitalisierung, vorgestellt.\nWir fordern in unserem Antrag – nämlich im\nSinne der Studierenden –, dass Lehrveranstaltungen weiterhin\nhybrid stattfinden, das heißt digital und physisch – nicht nur\nLehrveranstaltungen, sondern auch Prüfungen. Ja, Unis sind\nSozialräume, und das soll auch weiterhin sozusagen möglich sein, aber\nwir haben ja auch die Vorteile der digitalen Anwesenheit erfahren –\nStichwort Vereinbarkeitsgeschichten; wenn ich ein Kind zu Hause zu betreuen\nhabe oder pflegende Angehörige bin, habe ich trotzdem an der\nLehrveranstaltung oder der Prüfung teilnehmen können –,\nund ich denke mir, diese positiven Aspekte, die wir durch Covid auch erfahren\nkonnten, was eben mit diesen Hilfsmitteln möglich ist, die sollten wir\nbeibehalten.\nDas sowie die Zurverfügungstellung von digitalen\nEndgeräten fordern wir in diesem Antrag, weil halt nicht alle Studierenden\nsich das Studieren auch in dem Umfang leisten können. Es ist traurig, dass\nSie auch da nicht zugestimmt haben. – Danke schön. (Beifall bei der SPÖ.)\n");
    }

    #[tokio::test]
    async fn test_speech_extraction_too_long_kuensberg() {
        let speech = Speech {
            delegate_id: 5685,
            delegate_display_name: "Mag. Martina von Künsberg Sarre (N)".into(),
            opinion: Against,
            spoken_text_html_urls: vec![
                SpeechTextInfo {
                    site_start: Some(
                        253,
                    ),
                    site_end: Some(
                        254,
                    ),
                    document_url: "https://www.parlament.gv.at/dokument/XXVII/NRSITZ/117/fnameorig_1426448.html#RU_250839".into(),
                },
            ],
        };

        let mut extractor = TextExtractor::new();
        let text = extractor
            .full_speech_extraction(&speech.spoken_text_html_urls, &speech.delegate_display_name)
            .await
            .unwrap();
        assert_eq!(text, "\nAbgeordnete Mag. Martina Künsberg Sarre (NEOS): Herr Präsident! Frau Minister! Herr\nMinister! Hohes Haus! Ja, ich kann mich den Worten von Kollegin Niss nur\nanschließen. Wir brauchen Forschung, wir brauchen Investitionen in die\nForschung, und nicht zu knapp, es kann nicht genug geben. \nWir befinden uns immer noch in der Verfolgergruppe und sind\nimmer noch nicht in der Gruppe der Leader angekommen, obwohl wir jedes Jahr\nsehr, sehr viel Geld für die Forschung ausgeben. Sie, Herr Minister, haben\ndie Bereiche genannt. Ich rechne Ihnen immer hoch an, dass Sie kritisch sind\nund in vielen Bereichen auch das erwähnen, bei dem wir vielleicht nicht so\ngut sind. Das machen ja nicht alle Ihre Regierungskollegen. \nWas ist zu tun? – Ich meine, Sie geben einerseits\nviel mehr Geld für Forschung aus, beispielsweise beim FWF und für die\nExzellenzstrategie, aber Sie konterkarieren Ihre Bemühungen, indem der\nFonds Zukunft Österreich nach wie vor nicht aufgelegt ist –\nalso einerseits mehr reingeben und andererseits wird das, was es gegeben hat,\nnicht wieder aufgelegt. Das widerspricht und konterkariert sich. Sie legen\njetzt eine Zwischenfinanzierung oder Notfallsfinanzierung auf, was aber\nnicht darüber hinwegtäuschen kann, dass wir das endlich brauchen. Es\nist schade, dass der Finanzminister heute wieder nicht hier ist und sich\nder Diskussion stellt oder auch einmal sagt, warum das Geld eigentlich nicht\nkommt. Sie sitzen also immer da und sagen, es kommt, es kommt, aber es ist\njetzt bald das halbe Jahr vorbei. \nZu meinem Antrag, den Sie ablehnen, was ich\nsehr, sehr schade finde: Kollegin Niss hat vorhin gesagt, wenn wir in diesen\ndrei Bereichen aufholen, kommen wir endlich in die Leadergruppe. Das wollen wir\nhier alle, aber der Rat für Forschung und Technologieentwicklung hat\nsich in seiner Analyse der Zielsetzungen der FTI-Strategie auch mit dem\ngesamten Bildungsbereich auseinandergesetzt und kommt zum Schluss, dass vor\nallem \ndas Bildungssystem in seiner gesamten Breite eine Verbesserung der\nösterreichischen Innovationsperformance behindert. \nKollegin Blimlinger hat gesagt, dass Bildung\nin der FTI-Strategie abgebildet ist. – Ja, aber nicht vollkommen und\nnicht das gesamte Bildungssystem. Wir haben diesbezüglich einfach ein\nThema, und wenn über ein Viertel der 15-Jährigen nicht sinnerfassend\nlesen kann, dann haben wir ein Problem, und wenn Sie die IHS-Studie, die\nrausgekommen ist, durchlesen, dann sehen Sie, dass jene Kinder und\nJugendlichen, die vor der Krise schon benachteiligt waren, jetzt noch mehr\nbenachteiligt sind und abgehängt wurden. \nSie können also, finde ich, nicht immer\nnur sagen – und da wundert es mich, dass die Grünen einfach nur\nstill sind und nichts tun –, es ist eh alles in Ordnung und wir\nmachen weiter wie bisher. Wenn der Rat für Forschung und\nTechnologieentwicklung zu dem Schluss kommt, dass es wichtig wäre, das\ngesamte Bildungssystem als eine Säule in die FTI-Strategie aufzunehmen,\ndann finde ich es wichtig, dass Sie das auch umsetzen und\naufnehmen. – Danke. (Beifall bei den NEOS.) \n");
    }
}
