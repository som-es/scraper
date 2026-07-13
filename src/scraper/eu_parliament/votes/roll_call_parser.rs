use quick_xml::{
    events::{BytesStart, Event},
    Reader,
};

use crate::eu_parliament::votes::{EuDelegateVote, EuRollCallVote, EuVoteChoice, ParseError};

fn attr_string(e: &BytesStart, key: &[u8]) -> Result<Option<String>, ParseError> {
    for attr in e.attributes() {
        let attr = attr.map_err(|err| ParseError::Xml(err.to_string()))?;
        if attr.key.as_ref() == key {
            let value = attr
                .unescape_value()
                .map_err(|err| ParseError::Xml(err.to_string()))?;
            return Ok(Some(value.into_owned()));
        }
    }
    Ok(None)
}

fn required_u64(e: &BytesStart, key: &[u8], name: &'static str) -> Result<u64, ParseError> {
    let raw = attr_string(e, key)?.ok_or(ParseError::MissingAttribute(name))?;
    raw.trim()
        .parse::<u64>()
        .map_err(|err| ParseError::Number(format!("{name}: {err}")))
}

fn required_u32(e: &BytesStart, key: &[u8], name: &'static str) -> Result<u32, ParseError> {
    let raw = attr_string(e, key)?.ok_or(ParseError::MissingAttribute(name))?;
    raw.trim()
        .parse::<u32>()
        .map_err(|err| ParseError::Number(format!("{name}: {err}")))
}

pub fn parse_roll_call_votes_xml(xml: &str) -> Result<Vec<EuRollCallVote>, ParseError> {
    let mut reader = Reader::from_str(xml);

    let mut results: Vec<EuRollCallVote> = Vec::new();
    let mut current: Option<EuRollCallVote> = None;
    let mut current_choice: Option<EuVoteChoice> = None;
    let mut in_description = false;

    loop {
        let event = reader
            .read_event()
            .map_err(|e| ParseError::Xml(e.to_string()))?;

        match event {
            Event::Start(e) | Event::Empty(e) => match e.name().as_ref() {
                b"RollCallVote.Result" => {
                    let voting_id = required_u64(&e, b"Identifier", "Identifier").ok();
                    current = Some(EuRollCallVote {
                        voting_id,
                        description: None,
                        favor_count: 0,
                        against_count: 0,
                        abstention_count: 0,
                        votes: Vec::new(),
                    });
                }
                b"RollCallVote.Description.Text" => {
                    in_description = true;
                }
                b"Result.For" => {
                    if let Some(v) = current.as_mut() {
                        v.favor_count = required_u32(&e, b"Number", "Number")?;
                    }
                    current_choice = Some(EuVoteChoice::For);
                }
                b"Result.Against" => {
                    if let Some(v) = current.as_mut() {
                        v.against_count = required_u32(&e, b"Number", "Number")?;
                    }
                    current_choice = Some(EuVoteChoice::Against);
                }
                b"Result.Abstention" => {
                    if let Some(v) = current.as_mut() {
                        v.abstention_count = required_u32(&e, b"Number", "Number")?;
                    }
                    current_choice = Some(EuVoteChoice::Abstention);
                }
                b"PoliticalGroup.Member.Name" => {
                    if let (Some(v), Some(choice)) = (current.as_mut(), current_choice) {
                        let delegate_id = required_u64(&e, b"PersId", "PersId")?;
                        v.votes.push(EuDelegateVote {
                            delegate_id,
                            choice,
                        });
                    }
                }
                _ => {}
            },

            Event::Text(t) => {
                if in_description {
                    if let Some(v) = current.as_mut() {
                        let text = t.unescape().map_err(|e| ParseError::Xml(e.to_string()))?;
                        let trimmed = text.trim();
                        if !trimmed.is_empty() {
                            match v.description.as_mut() {
                                Some(existing) => {
                                    existing.push(' ');
                                    existing.push_str(trimmed);
                                }
                                None => v.description = Some(trimmed.to_string()),
                            }
                        }
                    }
                }
            }

            Event::End(e) => match e.name().as_ref() {
                b"RollCallVote.Description.Text" => in_description = false,
                b"Result.For" | b"Result.Against" | b"Result.Abstention" => {
                    current_choice = None;
                }
                b"RollCallVote.Result" => {
                    if let Some(v) = current.take() {
                        results.push(v);
                    }
                }
                _ => {}
            },

            Event::Eof => break,
            _ => {}
        }
    }

    Ok(results)
}

pub fn parse_roll_call_vote_xml(xml: &str) -> Result<EuRollCallVote, ParseError> {
    parse_roll_call_votes_xml(xml)?
        .into_iter()
        .next()
        .ok_or(ParseError::NoResult)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"
<RollCallVote.Result Identifier="168997" DlvId="946508" Date="2024-04-25 12:19:38">
<RollCallVote.Description.Text> C9-0010/2024 - Proposition de la Commission</RollCallVote.Description.Text>
<Result.For Number="511">
<Result.PoliticalGroup.List Identifier="PPE">
<PoliticalGroup.Member.Name MepId="1000" PersId="500001">Alpha</PoliticalGroup.Member.Name>
<PoliticalGroup.Member.Name MepId="1001" PersId="500002">Beta</PoliticalGroup.Member.Name>
</Result.PoliticalGroup.List>
</Result.For>
<Result.Against Number="1">
<Result.PoliticalGroup.List Identifier="NI">
<PoliticalGroup.Member.Name MepId="6630" PersId="125025">de Graaff</PoliticalGroup.Member.Name>
</Result.PoliticalGroup.List>
</Result.Against>
<Result.Abstention Number="21">
<Result.PoliticalGroup.List Identifier="ECR">
<PoliticalGroup.Member.Name MepId="5472" PersId="28353">Fotyga</PoliticalGroup.Member.Name>
<PoliticalGroup.Member.Name MepId="6911" PersId="197518">Jurgiel</PoliticalGroup.Member.Name>
</Result.PoliticalGroup.List>
<Result.PoliticalGroup.List Identifier="The Left">
<PoliticalGroup.Member.Name MepId="6873" PersId="187917">Botenga</PoliticalGroup.Member.Name>
</Result.PoliticalGroup.List>
</Result.Abstention>
</RollCallVote.Result>
"#;

    #[test]
    fn parses_metadata() {
        let v = parse_roll_call_vote_xml(SAMPLE).unwrap();
        assert_eq!(v.voting_id, Some(168997));
        assert_eq!(
            v.description.as_deref(),
            Some("C9-0010/2024 - Proposition de la Commission")
        );
        assert_eq!(v.favor_count, 511);
        assert_eq!(v.against_count, 1);
        assert_eq!(v.abstention_count, 21);
    }

    #[test]
    fn flattens_votes_with_persid_and_choice() {
        let v = parse_roll_call_vote_xml(SAMPLE).unwrap();
        assert_eq!(v.votes.len(), 6);
        assert!(v.votes.contains(&EuDelegateVote {
            delegate_id: 500001,
            choice: EuVoteChoice::For
        }));
        assert!(v.votes.contains(&EuDelegateVote {
            delegate_id: 125025,
            choice: EuVoteChoice::Against
        }));
        assert!(v.votes.contains(&EuDelegateVote {
            delegate_id: 187917,
            choice: EuVoteChoice::Abstention
        }));
    }

    #[test]
    fn handles_multiple_results() {
        let doc = format!("<Root>{SAMPLE}{SAMPLE}</Root>");
        let all = parse_roll_call_votes_xml(&doc).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], all[1]);
    }
}
