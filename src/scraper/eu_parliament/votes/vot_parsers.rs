use quick_xml::encoding::Decoder;
use quick_xml::events::{BytesStart, Event};
use quick_xml::Reader;
use serde_derive::{Deserialize, Serialize};

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum VotingType {
    Amendment,  // AMENDMENT
    TitleBlock, // TITLE_BLOCK — the text "as a whole"
    Other(String),
}

impl VotingType {
    pub fn from_xml(s: &str) -> Self {
        match s {
            "AMENDMENT" => VotingType::Amendment,
            "TITLE_BLOCK" => VotingType::TitleBlock,
            other => VotingType::Other(other.to_owned()),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum VoteType {
    Report,     // REPORT
    Resolution, // RESOLUTION
    Proposal,   // PROPOSAL — legislative (Commission) proposal
    Info,       // INFO — non-vote section (corrections/notes)
    Eugi,       // EUGI
    Actes,      // ACTES
    Other(String),
}

impl VoteType {
    pub fn from_xml(s: &str) -> Self {
        match s {
            "REPORT" => VoteType::Report,
            "RESOLUTION" => VoteType::Resolution,
            "PROPOSAL" => VoteType::Proposal,
            "INFO" => VoteType::Info,
            "EUGI" => VoteType::Eugi,
            "ACTES" => VoteType::Actes,
            other => VoteType::Other(other.to_owned()),
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum VoteResult {
    Adopted,   // ADOPTED
    Rejected,  // REJECTED
    Lapsed,    // LAPSED fell because an earlier, incompatible vote passed
    Withdrawn, // WITHDRAWN
    Other(String),
}

impl VoteResult {
    pub fn from_xml(s: &str) -> Self {
        match s {
            "ADOPTED" => VoteResult::Adopted,
            "REJECTED" => VoteResult::Rejected,
            "LAPSED" => VoteResult::Lapsed,
            "WITHDRAWN" => VoteResult::Withdrawn,
            other => VoteResult::Other(other.to_owned()),
        }
    }

    pub fn is_adopted(&self) -> bool {
        matches!(self, VoteResult::Adopted)
    }
}

#[derive(Clone, PartialEq, Eq, Hash, Debug, Serialize, Deserialize)]
pub enum VoteResultType {
    RollCall,     // ROLL_CALL — individual MEP positions recorded
    Electronical, // ELECTRONICAL
    RaiseHand,    // RAISE_HAND
    Secret,       // SECRET
    Other(String),
}

impl VoteResultType {
    pub fn from_xml(s: &str) -> Self {
        match s {
            "ROLL_CALL" => VoteResultType::RollCall,
            "ELECTRONICAL" => VoteResultType::Electronical,
            "RAISE_HAND" => VoteResultType::RaiseHand,
            "SECRET" => VoteResultType::Secret,
            other => VoteResultType::Other(other.to_owned()),
        }
    }

    pub fn is_roll_call(&self) -> bool {
        matches!(self, VoteResultType::RollCall)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VotEntry {
    pub label: String,
    pub votings: Vec<VotingRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct VotingRecord {
    pub voting_id: u64,
    pub voting_type: Option<VotingType>, // from <voting type>
    pub vote_type: Option<VoteType>,     // from the parent <vote type>
    pub result: VoteResult,
    pub title: Option<String>,
    pub result_type: VoteResultType,
    pub vote_timestamp: Option<String>, // raw RFC3339, e.g. "2024-04-25T12:40:48.000+02:00"
    pub amendment_subject: Option<String>,
    pub amendment_number: Option<String>,
    pub amendment_author: Option<String>,
    pub favor_count: Option<u32>,
    pub against_count: Option<u32>,
    pub abstention_count: Option<u32>,
    pub voters_count: Option<u32>,
}

fn attr_str(e: &BytesStart, key: &[u8], decoder: Decoder) -> Option<String> {
    e.attributes()
        .flatten()
        .find(|a| a.key.as_ref() == key)
        .and_then(|a| a.decode_and_unescape_value(decoder).ok())
        .map(|c| c.into_owned())
}

fn attr_u32(e: &BytesStart, key: &[u8], decoder: Decoder) -> Option<u32> {
    attr_str(e, key, decoder).and_then(|s| s.trim().parse().ok())
}

fn build_voting_record(
    e: &BytesStart,
    decoder: Decoder,
    vote_type: Option<&str>,
) -> Option<VotingRecord> {
    let voting_id: u64 = attr_str(e, b"votingId", decoder)?.trim().parse().ok()?;
    let result = VoteResult::from_xml(attr_str(e, b"result", decoder)?.as_str());
    let result_type = VoteResultType::from_xml(attr_str(e, b"resultType", decoder)?.as_str());

    Some(VotingRecord {
        voting_id,
        voting_type: attr_str(e, b"type", decoder)
            .as_deref()
            .map(VotingType::from_xml),
        vote_type: vote_type.map(VoteType::from_xml),
        result,
        result_type,
        vote_timestamp: attr_str(e, b"voteTimestamp", decoder),
        title: None,
        amendment_subject: None,
        amendment_number: None,
        amendment_author: None,
        favor_count: attr_u32(e, b"numberFavor", decoder),
        against_count: attr_u32(e, b"numberAgainst", decoder),
        abstention_count: attr_u32(e, b"numberAbstention", decoder),
        voters_count: attr_u32(e, b"votersCount", decoder),
    })
}

#[derive(Copy, Clone)]
enum AmendmentField {
    Subject,
    Number,
    Author,
}

pub fn parse_vot_xml(xml: &str) -> Vec<VotEntry> {
    let mut reader = Reader::from_str(xml);
    reader.config_mut().trim_text(true);
    let decoder = reader.decoder();

    let mut entries = Vec::new();

    let mut in_vote = false;
    let mut in_voting = false;
    let mut in_vote_label = false;
    let mut in_voting_title = false;
    let mut amendment_field: Option<AmendmentField> = None;

    let mut current_vote_type: Option<String> = None; // parent <vote type="...">
    let mut current_label = String::new();
    let mut current_title = String::new();
    let mut current_votings: Vec<VotingRecord> = Vec::new();
    let mut current_voting: Option<VotingRecord> = None;

    loop {
        match reader.read_event() {
            Ok(Event::Start(e)) => match e.name().as_ref() {
                b"vote" => {
                    in_vote = true;
                    current_label.clear();
                    current_votings.clear();
                    current_vote_type = attr_str(&e, b"type", decoder);
                }
                b"label" if in_vote && !in_voting => {
                    in_vote_label = true;
                }
                b"title" if in_vote && in_voting => {
                    in_voting_title = true;
                }
                b"voting" if in_vote => {
                    in_voting = true;
                    current_voting = build_voting_record(&e, decoder, current_vote_type.as_deref());
                }
                // Flags are set ONLY on Start, so a self-closing <amendmentSubject/>
                // (an Empty event) can't leave a flag stuck open.
                b"amendmentSubject" if in_voting => amendment_field = Some(AmendmentField::Subject),
                b"amendmentNumber" if in_voting => amendment_field = Some(AmendmentField::Number),
                b"amendmentAuthor" if in_voting => amendment_field = Some(AmendmentField::Author),
                _ => {}
            },

            // Self-closing tags. Empty <amendment*/> => value stays None.
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"voting" && in_vote {
                    if let Some(rec) =
                        build_voting_record(&e, decoder, current_vote_type.as_deref())
                    {
                        current_votings.push(rec);
                    }
                }
            }

            Ok(Event::Text(e)) if in_vote_label && amendment_field.is_none() => {
                current_label.push_str(&e.unescape().unwrap_or_default());
            }
            Ok(Event::Text(e)) if in_voting_title && amendment_field.is_none() => {
                if let Some(v) = current_voting.as_mut() {
                    let text = e.unescape().unwrap_or_default().into_owned();

                    match &mut v.title {
                        Some(existing) => existing.push_str(&text),
                        None => v.title = Some(text),
                    }
                }
            }

            Ok(Event::Text(e)) if amendment_field.is_some() => {
                if let Some(v) = current_voting.as_mut() {
                    let text = e.unescape().unwrap_or_default().into_owned();
                    let slot = match amendment_field {
                        Some(AmendmentField::Subject) => &mut v.amendment_subject,
                        Some(AmendmentField::Number) => &mut v.amendment_number,
                        Some(AmendmentField::Author) => &mut v.amendment_author,
                        None => unreachable!(),
                    };
                    match slot {
                        Some(existing) => existing.push_str(&text),
                        None => *slot = Some(text),
                    }
                }
            }

            Ok(Event::End(e)) => match e.name().as_ref() {
                b"label" if in_vote_label => in_vote_label = false,
                b"title" if in_voting_title => in_voting_title = false,
                b"amendmentSubject" | b"amendmentNumber" | b"amendmentAuthor" => {
                    amendment_field = None
                }
                b"voting" => {
                    in_voting = false;
                    if let Some(v) = current_voting.take() {
                        current_votings.push(v);
                    }
                }
                b"vote" => {
                    in_vote = false;
                    if !current_votings.is_empty() {
                        entries.push(VotEntry {
                            label: std::mem::take(&mut current_label),
                            votings: std::mem::take(&mut current_votings),
                        });
                    }
                }
                _ => {}
            },

            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
    }

    entries
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<votes>
<vote listIndex="0" dlvId="1" type="REPORT" author="Freund">
<label>A9-0181/2024 – Daniel Freund</label>
<votings>
<voting type="AMENDMENT" votingId="169150" result="REJECTED" resultType="RAISE_HAND" voteTimestamp="2024-04-25T16:07:16.955+02:00">
<title>comission proposal</title>
<amendmentSubject>Recital G</amendmentSubject>
<amendmentNumber>62</amendmentNumber>
<amendmentAuthor>PPE</amendmentAuthor>
</voting>
<voting type="TITLE_BLOCK" votingId="168984" result="ADOPTED" resultType="ROLL_CALL" numberAbstention="23" numberAgainst="216" numberFavor="301" votersCount="540" voteTimestamp="2024-04-25T12:40:48.000+02:00">
<amendmentSubject/>
<rcv type="RCV"><value>RCV</value></rcv>
<observations status="verbatim">301, 216, 23</observations>
</voting>
<voting type="TITLE_BLOCK" votingId="9999" result="LAPSED" resultType="SECRET" voteTimestamp="2024-04-25T12:41:00.000+02:00"/>
<voting type="TITLE_BLOCK"></voting>
</votings>
</vote>
<vote listIndex="1" type="PROPOSAL">
<label>Commission proposal (as a whole)</label>
<votings>
<voting type="WEIRD_NEW_TYPE" votingId="5000" result="FLOOR" resultType="ELECTRONICAL" voteTimestamp="2024-04-25T12:50:00.000+02:00"/>
</votings>
</vote>
</votes>"#;

    #[test]
    fn faithful_enums() {
        let entries = parse_vot_xml(SAMPLE);
        assert_eq!(entries.len(), 2);

        let rep = &entries[0];
        assert_eq!(rep.votings.len(), 3); // header voting (no votingId) skipped

        let a = &rep.votings[0];
        assert_eq!(a.voting_id, 169150);
        assert_eq!(a.title.as_deref().unwrap(), "comission proposal");
        assert_eq!(a.voting_type, Some(VotingType::Amendment));
        assert_eq!(a.vote_type, Some(VoteType::Report));
        assert_eq!(a.result, VoteResult::Rejected);
        assert_eq!(a.result_type, VoteResultType::RaiseHand);
        assert_eq!(a.amendment_subject.as_deref(), Some("Recital G"));
        assert_eq!(a.amendment_number.as_deref(), Some("62"));
        assert_eq!(a.amendment_author.as_deref(), Some("PPE"));

        let w = &rep.votings[1];
        assert_eq!(w.voting_type, Some(VotingType::TitleBlock));
        assert_eq!(w.vote_type, Some(VoteType::Report));
        assert_eq!(w.result, VoteResult::Adopted);
        assert!(w.result.is_adopted() && w.result_type.is_roll_call());
        assert_eq!(w.favor_count, Some(301));
        assert_eq!(w.voters_count, Some(540));
        assert_eq!(w.amendment_subject, None);

        let l = &rep.votings[2];
        assert_eq!(l.result, VoteResult::Lapsed);
        assert_eq!(l.result_type, VoteResultType::Secret);

        // Unknown strings are preserved via Other(_), not lost:
        let p = &entries[1].votings[0];
        assert_eq!(p.vote_type, Some(VoteType::Proposal));
        assert_eq!(
            p.voting_type,
            Some(VotingType::Other("WEIRD_NEW_TYPE".into()))
        );
        assert_eq!(p.result, VoteResult::Other("FLOOR".into()));
        assert_eq!(p.result_type, VoteResultType::Electronical);
    }
}
